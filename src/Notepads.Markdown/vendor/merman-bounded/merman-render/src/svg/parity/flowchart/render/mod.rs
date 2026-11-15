//! Flowchart SVG renderer (core drawing routines).

mod cluster;
mod edge_label;
mod edge_path;
pub(in crate::svg::parity) mod node;
mod root;

pub(in crate::svg::parity) use cluster::render_flowchart_cluster;
pub(in crate::svg::parity) use edge_label::render_flowchart_edge_label;
pub(super) use edge_label::resolve_flowchart_edge_label_position;
pub(super) use edge_path::render_flowchart_edge_path;
pub(super) use node::render_flowchart_node;
pub(super) use root::{
    FlowchartRootRenderSession, flowchart_elk_renders_empty_subgraph_as_cluster,
    render_flowchart_elk_root_groups, render_flowchart_root,
};
