use super::*;
use rustc_hash::{FxHashMap, FxHashSet};

mod agentflow;
mod css;
mod defs;
mod document;
mod edge;
mod edge_geom;
mod hierarchy;
mod label;
mod render;
mod render_config;
mod render_input;
mod style;
mod swimlane;
mod types;
mod util;
mod viewbox;
mod viewbox_node_bounds;

use css::*;
use edge::*;
pub(in crate::svg::parity::flowchart) use edge_geom::{
    FlowchartEdgePathGeomRequest, flowchart_compute_edge_path_geom,
};
use hierarchy::*;
pub(super) use label::*;
pub(super) use style::*;

use render::{
    FlowchartRootRenderSession, render_flowchart_edge_path, render_flowchart_elk_root_groups,
    render_flowchart_node, render_flowchart_root,
};
pub(super) use render::{render_flowchart_cluster, render_flowchart_edge_label};
use types::*;
use util::{HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR, OptionalStyleAttr, OptionalStyleXmlAttr};

// Flowchart SVG renderer implementation (split from parity.rs).

// Mermaid's `createText(...)` defaults its `width` argument to 200. Flowchart edge labels call
// `createText(...)` without overriding that width, so keep edge label wrapping/max-width fixed at
// 200px (independent of `flowchart.wrappingWidth`).
pub(in crate::svg::parity::flowchart) const FLOWCHART_EDGE_LABEL_WRAP_WIDTH: f64 = 200.0;

// In flowchart SVG emission, many attribute payloads are known to be short-lived (colors, inline
// `d` strings, etc). Avoid allocating an owned `String` for attribute escaping by default.
#[inline]
fn escape_attr(text: &str) -> super::util::EscapeAttrDisplay<&str> {
    escape_attr_display(text)
}

pub(in crate::svg::parity::flowchart) fn flowchart_config_look(
    config: &merman_core::MermaidConfig,
) -> &str {
    flowchart_config_diagram_look(config).as_str()
}

pub(in crate::svg::parity::flowchart) fn flowchart_config_diagram_look(
    config: &merman_core::MermaidConfig,
) -> crate::config::DiagramLook<'_> {
    crate::config::mermaid_config_diagram_look(config)
}

// Entry points (split from parity.rs).

mod svg_emit;
pub(super) use svg_emit::render_flowchart_svg_artifact;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(super) use swimlane::render_swimlane_svg_artifact;

#[cfg(all(test, feature = "layout-elk"))]
mod browser_measurement_tests;

/// Shared by the title element and its stylesheet; Agentflow owns its title class.
fn title_css_class(diagram_type: &str) -> &'static str {
    if diagram_type == "agentflow" {
        "agentflowTitleText"
    } else {
        "flowchartTitleText"
    }
}
