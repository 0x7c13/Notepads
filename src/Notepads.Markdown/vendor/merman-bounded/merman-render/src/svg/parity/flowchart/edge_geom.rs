//! Flowchart edge geometry helpers.
//!
//! This module is a façade to keep the flowchart renderer organized.

use super::*;

mod basis;
mod boundary;
mod compute;
mod curve_path;
mod elk_points;
mod intersect;
mod line_with_offset;
mod rect_clip;
mod terminal_jogs;
mod trace;
pub(super) use compute::{finish_edge_route, prepare_edge_route};
pub(super) use terminal_jogs::{separate_edge_labels, straighten_edge_terminals};

pub(super) use crate::svg::parity::edge_path::maybe_fix_corners;
pub(super) use basis::maybe_remove_redundant_cluster_run_point;
pub(super) use boundary::{BoundaryNode, boundary_for_cluster, boundary_for_node};
pub(super) use curve_path::curve_path_d_and_bounds;
pub(super) use elk_points::{
    ElkEndpointAdapterCorners, apply_flowchart_elk_endpoint_cutter, missing_section_label_position,
    missing_section_points,
};
pub(super) use intersect::{
    force_intersect_for_layout_shape, intersect_for_layout_shape, is_rounded_intersect_shift_shape,
};
pub(super) use line_with_offset::{
    arrow_types_for_edge, collapse_short_terminal_marker_stub, line_with_offset_for_edge_type,
    rounded_line_with_marker_offsets_for_edge_type,
};
pub(super) use rect_clip::{cut_path_at_intersect_into, dedup_consecutive_points_into};
pub(super) use trace::{FlowchartEdgeTraceInput, record_flowchart_edge_trace};

pub(super) struct ClippedEdgeRoute {
    base_points: Vec<crate::model::LayoutPoint>,
    points: Vec<crate::model::LayoutPoint>,
    origin_x: f64,
    origin_y: f64,
    label: Option<crate::model::LayoutLabel>,
    elk_endpoint_adapters: ElkEndpointAdapterCorners,
}

#[derive(Clone, Copy)]
pub(in crate::svg::parity::flowchart) struct FlowchartEdgePathGeomRequest<'a> {
    pub(super) ctx: &'a FlowchartRenderCtx<'a>,
    pub(super) edge: &'a crate::flowchart::FlowEdge,
    pub(super) origin_x: f64,
    pub(super) origin_y: f64,
    pub(super) trace_enabled: bool,
}

pub(in crate::svg::parity::flowchart) fn flowchart_compute_edge_path_geom(
    request: FlowchartEdgePathGeomRequest<'_>,
    scratch: &mut FlowchartEdgeDataPointsScratch,
) -> Option<FlowchartEdgePathGeom> {
    let route = prepare_edge_route(request, scratch)?;
    finish_edge_route(request, route, scratch)
}
