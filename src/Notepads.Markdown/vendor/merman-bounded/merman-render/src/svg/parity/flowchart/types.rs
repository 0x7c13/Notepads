// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
//! Shared flowchart rendering types.
//!
//! This module keeps `flowchart.rs` slimmer by housing context structs and scratch buffers that
//! are used across flowchart SVG emission (rendering + viewBox computation).

use super::super::*;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy)]
pub(in crate::svg::parity::flowchart) struct FlowchartEmitCheckpoint<'a> {
    checkpoint: &'a dyn Fn() -> crate::Result<()>,
}

impl<'a> FlowchartEmitCheckpoint<'a> {
    pub(in crate::svg::parity::flowchart) fn new(
        checkpoint: &'a dyn Fn() -> crate::Result<()>,
    ) -> Self {
        Self { checkpoint }
    }

    pub(in crate::svg::parity::flowchart) fn checkpoint(self) -> crate::Result<()> {
        (self.checkpoint)()
    }
}

pub(in crate::svg::parity) struct FlowchartRenderCtx<'a> {
    pub(in crate::svg::parity::flowchart) model: &'a crate::flowchart::FlowchartRenderModelRef<'a>,
    pub(in crate::svg::parity::flowchart) diagram_id: SvgDiagramId<'a>,
    pub(in crate::svg::parity::flowchart) diagram_type: &'a str,
    pub(in crate::svg::parity::flowchart) tx: f64,
    pub(in crate::svg::parity::flowchart) ty: f64,
    pub(in crate::svg::parity::flowchart) measurer: &'a dyn TextMeasurer,
    pub(in crate::svg::parity::flowchart) config: &'a merman_core::MermaidConfig,
    pub(in crate::svg::parity::flowchart) hand_drawn_seed: roughr::core::RoughRandomness,
    pub(in crate::svg::parity::flowchart) work_meter: &'a crate::resources::OperationWorkMeter,
    pub(in crate::svg::parity::flowchart) emit: FlowchartEmitCheckpoint<'a>,
    pub(in crate::svg::parity::flowchart) math_renderer:
        Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(in crate::svg::parity::flowchart) svg_label_sidecar:
        Option<&'a crate::flowchart::FlowchartSvgLabelSidecar>,
    pub(in crate::svg::parity::flowchart) icon_registry: Option<&'a crate::svg::IconRegistry>,
    pub(in crate::svg::parity::flowchart) icon_scope_prefix:
        Option<crate::svg::icon_registry::IconIdScopePrefix>,
    pub(in crate::svg::parity::flowchart) security_level_loose: bool,
    pub(in crate::svg::parity::flowchart) node_html_labels: bool,
    pub(in crate::svg::parity::flowchart) edge_html_labels: bool,
    pub(in crate::svg::parity::flowchart) swimlane_title_html_labels: bool,
    pub(in crate::svg::parity::flowchart) uses_elk_adapter_dom: bool,
    pub(in crate::svg::parity::flowchart) class_defs: &'a merman_core::native_collections::OrderedMap<String, Vec<String>>,
    pub(in crate::svg::parity::flowchart) node_border_color: String,
    pub(in crate::svg::parity::flowchart) node_fill_color: String,
    pub(in crate::svg::parity::flowchart) edge_corner_radius: f64,
    pub(in crate::svg::parity::flowchart) edge_label_padding: f64,
    pub(in crate::svg::parity::flowchart) compact_edge_corners: bool,
    pub(in crate::svg::parity::flowchart) default_edge_interpolate: String,
    pub(in crate::svg::parity::flowchart) default_edge_style: Vec<String>,
    pub(in crate::svg::parity::flowchart) trace_edge_id: Option<&'a str>,
    pub(in crate::svg::parity::flowchart) trace_collector:
        Option<&'a crate::svg::FlowchartEdgeTraceCollector>,
    pub(in crate::svg::parity::flowchart) subgraph_order: Vec<&'a str>,
    pub(in crate::svg::parity::flowchart) edge_order: Vec<&'a str>,
    pub(in crate::svg::parity::flowchart) nodes_by_id:
        FxHashMap<&'a str, &'a crate::flowchart::FlowNode>,
    pub(in crate::svg::parity::flowchart) edges_by_id:
        FxHashMap<&'a str, &'a crate::flowchart::FlowEdge>,
    pub(in crate::svg::parity::flowchart) subgraphs_by_id:
        FxHashMap<&'a str, &'a crate::flowchart::FlowSubgraph>,
    pub(in crate::svg::parity::flowchart) subgraph_indices_by_id: FxHashMap<&'a str, usize>,
    pub(in crate::svg::parity::flowchart) subgraph_ids_with_children: FxHashSet<&'a str>,
    pub(in crate::svg::parity::flowchart) tooltips: &'a merman_core::native_collections::NativeHashMap<String, String>,
    pub(in crate::svg::parity::flowchart) recursive_clusters: FxHashSet<&'a str>,
    pub(in crate::svg::parity::flowchart) parent: FxHashMap<&'a str, &'a str>,
    pub(in crate::svg::parity::flowchart) layout_nodes_by_id: FxHashMap<&'a str, &'a LayoutNode>,
    pub(in crate::svg::parity::flowchart) layout_edges_by_id:
        FxHashMap<&'a str, &'a crate::model::LayoutEdge>,
    pub(in crate::svg::parity::flowchart) layout_clusters_by_id:
        FxHashMap<&'a str, &'a LayoutCluster>,
    pub(in crate::svg::parity::flowchart) swimlane_direction:
        Option<crate::model::SwimlaneDirection>,
    pub(in crate::svg::parity::flowchart) swimlane_lanes_by_id:
        FxHashMap<&'a str, &'a crate::model::SwimlaneLaneLayout>,
    pub(in crate::svg::parity::flowchart) swimlane_edge_label_edges_by_node_id:
        FxHashMap<&'a str, &'a crate::flowchart::FlowEdge>,
    pub(in crate::svg::parity::flowchart) dom_node_order_by_root:
        &'a std::collections::HashMap<String, Vec<String>>,
    pub(in crate::svg::parity::flowchart) node_dom_index: FxHashMap<&'a str, usize>,
    pub(in crate::svg::parity::flowchart) node_padding: f64,
    pub(in crate::svg::parity::flowchart) wrapping_width: f64,
    pub(in crate::svg::parity::flowchart) node_wrap_mode: WrapMode,
    pub(in crate::svg::parity::flowchart) edge_wrap_mode: WrapMode,
    pub(in crate::svg::parity::flowchart) text_style: TextStyle,
    pub(in crate::svg::parity::flowchart) html_label_text_style: TextStyle,
}

impl FlowchartRenderCtx<'_> {
    pub(in crate::svg::parity::flowchart) fn subgraph_has_children(&self, id: &str) -> bool {
        self.subgraph_ids_with_children.contains(id)
    }

    pub(in crate::svg::parity::flowchart) fn is_subgraph_collapsed(&self, id: &str) -> bool {
        self.model.is_subgraph_collapsed(id)
    }

    pub(in crate::svg::parity::flowchart) fn checkpoint_emit(&self) -> crate::Result<()> {
        self.emit.checkpoint()
    }
}

#[derive(Debug, Default, Clone)]
pub(in crate::svg::parity::flowchart) struct FlowchartRenderDetails {
    pub(in crate::svg::parity::flowchart) root_calls: u32,
    pub(in crate::svg::parity::flowchart) clusters: std::time::Duration,
    pub(in crate::svg::parity::flowchart) edges_select: std::time::Duration,
    pub(in crate::svg::parity::flowchart) edge_paths: std::time::Duration,
    pub(in crate::svg::parity::flowchart) edge_labels: std::time::Duration,
    pub(in crate::svg::parity::flowchart) dom_order: std::time::Duration,
    pub(in crate::svg::parity::flowchart) nodes: std::time::Duration,
    pub(in crate::svg::parity::flowchart) node_style_compile: std::time::Duration,
    pub(in crate::svg::parity::flowchart) node_roughjs: std::time::Duration,
    pub(in crate::svg::parity::flowchart) node_roughjs_calls: u32,
    pub(in crate::svg::parity::flowchart) node_label_html: std::time::Duration,
    pub(in crate::svg::parity::flowchart) node_label_html_calls: u32,
    pub(in crate::svg::parity::flowchart) nested_roots: std::time::Duration,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_lca: std::time::Duration,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_offsets: std::time::Duration,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_geom: std::time::Duration,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_bbox_union: std::time::Duration,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_geom_calls: u32,
    pub(in crate::svg::parity::flowchart) viewbox_edge_curve_geom_skipped_bounds: u32,
}

#[derive(Default)]
pub(in crate::svg::parity::flowchart) struct FlowchartEdgeDataPointsScratch {
    pub(in crate::svg::parity::flowchart) json: String,
    pub(in crate::svg::parity::flowchart) edge_style: String,
    pub(in crate::svg::parity::flowchart) ryu: ryu_js::Buffer,
    pub(in crate::svg::parity::flowchart) local_points: Vec<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) tmp_points_a: Vec<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) tmp_points_b: Vec<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) tmp_points_c: Vec<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) tmp_points_rev: Vec<crate::model::LayoutPoint>,
}

#[derive(Debug, Clone)]
pub(in crate::svg::parity::flowchart) struct FlowchartEdgePathGeom {
    pub(in crate::svg::parity::flowchart) d: String,
    pub(in crate::svg::parity::flowchart) pb: Option<path_bounds::SvgPathBounds>,
    /// Exact point list serialized into `data-points` by Mermaid's `insertEdge`.
    pub(in crate::svg::parity::flowchart) data_points: Vec<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) data_points_b64: String,
    pub(in crate::svg::parity::flowchart) original_path_length: Option<f64>,
    pub(in crate::svg::parity::flowchart) path_length: Option<f64>,
    /// A crossing triggered after-paint path rewriting, even if its visible jump was suppressed.
    pub(in crate::svg::parity::flowchart) line_hop_applied: bool,
    /// Input to insertEdge after layout postprocessing, before paint-time clipping.
    pub(in crate::svg::parity::flowchart) original_label_path_points:
        Vec<crate::model::LayoutPoint>,
    /// Label polyline after endpoint/cluster clipping and before curve preprocessing.
    pub(in crate::svg::parity::flowchart) label_path_points: Vec<crate::model::LayoutPoint>,
    /// ELK main label after terminal-run projection and paired-label separation, in local coordinates.
    pub(in crate::svg::parity::flowchart) label_anchor: Option<crate::model::LayoutPoint>,
    pub(in crate::svg::parity::flowchart) label_path_was_explicitly_updated: bool,
    /// Actual path emitted by `insertEdge`; hand-drawn edges differ from the logical curve `d`.
    pub(in crate::svg::parity::flowchart) emitted_d_for_label: Option<String>,
    pub(in crate::svg::parity::flowchart) bounds_skipped_for_viewbox: bool,
}

#[derive(Debug, Clone)]
pub(in crate::svg::parity) struct FlowchartEdgePathCacheEntry {
    pub(in crate::svg::parity::flowchart) origin_x: f64,
    pub(in crate::svg::parity::flowchart) origin_y: f64,
    pub(in crate::svg::parity::flowchart) abs_top_transform: f64,
    pub(in crate::svg::parity::flowchart) geom: FlowchartEdgePathGeom,
}

#[inline]
pub(in crate::svg::parity::flowchart) fn detail_guard<'a>(
    timing: timing::RenderTiming,
    dst: &'a mut std::time::Duration,
) -> Option<timing::TimingGuard<'a>> {
    timing.section(dst)
}

#[derive(Debug, Clone, Copy)]
pub(in crate::svg::parity::flowchart) struct FlowchartRootOffsets {
    pub(in crate::svg::parity::flowchart) origin_x: f64,
    pub(in crate::svg::parity::flowchart) origin_y: f64,
    pub(in crate::svg::parity::flowchart) abs_top_transform: f64,
}
