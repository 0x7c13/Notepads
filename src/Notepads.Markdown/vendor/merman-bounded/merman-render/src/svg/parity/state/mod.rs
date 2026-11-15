use super::*;
use rustc_hash::FxHashMap;
mod context;
mod edge;
mod node;
mod rough_cache;
pub(in crate::svg::parity) mod roughjs;
mod style;
mod viewport;

pub(super) use super::roughjs_common::roughjs_paths_for_rect;
pub(super) use super::roughjs_common::{
    RoughRectSpec as StateRoughRectSpec, roughjs_circle_path_d,
};

use roughjs::{
    mermaid_choice_diamond_path_data, mermaid_rounded_rect_path_data, roughjs_paths_for_svg_path,
};

// State diagram SVG renderer implementation (split from parity.rs).

use context::*;
use edge::*;
use node::*;
use rough_cache::*;
use style::*;
use viewport::*;

type StateSvgModel = merman_core::diagrams::state::StateDiagramRenderModel;
type StateSvgState = merman_core::diagrams::state::StateDiagramRenderState;
type StateSvgLink = merman_core::diagrams::state::StateDiagramRenderLink;
type StateSvgLinks = merman_core::diagrams::state::StateDiagramRenderLinks;
type StateSvgNode = merman_core::diagrams::state::StateDiagramRenderNode;
type StateSvgEdge = merman_core::diagrams::state::StateDiagramRenderEdge;
struct StateRenderCtx<'a> {
    diagram_id: SvgDiagramId<'a>,
    diagram_look: String,
    palette_size: usize,
    uses_elk_adapter_dom: bool,
    elk_edge_paths: &'a std::collections::HashMap<String, Vec<crate::model::LayoutPoint>>,
    elk_line_hop_paths: FxHashMap<String, String>,
    hand_drawn_seed: roughr::core::RoughRandomness,
    html_labels: bool,
    html_label_wrapping_width: f64,
    label_min_width: f64,
    state_padding: f64,
    node_order: Vec<&'a str>,
    nodes_by_id: FxHashMap<&'a str, &'a StateSvgNode>,
    layout_nodes_by_id: FxHashMap<&'a str, &'a LayoutNode>,
    layout_edges_by_id: FxHashMap<&'a str, &'a crate::model::LayoutEdge>,
    layout_clusters_by_id: FxHashMap<&'a str, &'a LayoutCluster>,
    parent: FxHashMap<&'a str, &'a str>,
    nested_roots: std::collections::BTreeSet<String>,
    hidden_prefixes: Vec<String>,
    security_level_loose: bool,
    links: &'a std::collections::HashMap<String, StateSvgLinks>,
    states: &'a std::collections::HashMap<String, StateSvgState>,
    edges: &'a [StateSvgEdge],
    include_edges: bool,
    include_nodes: bool,
    measurer: &'a dyn TextMeasurer,
    text_style: crate::text::TextStyle,
    theme_defaults: StateThemeDefaults,
    rough_cache: StateRoughCache,
}

mod render;
pub(super) use render::render_state_diagram_svg_model;
