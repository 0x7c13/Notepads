//! State diagram (stateDiagram-v2) layout.
//!
//! Source semantics: Mermaid 12.0.0.

type StateDiagramModel = merman_core::diagrams::state::StateDiagramRenderModel;
type StateNode = merman_core::diagrams::state::StateDiagramRenderNode;

mod label;
pub(crate) use label::{
    measure_state_markdown_label, state_edge_label_xhtml, state_node_label_xhtml,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RectWithTitleGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) label_x: f64,
    pub(crate) label_y: f64,
    pub(crate) title_x: f64,
    pub(crate) description_x: f64,
    pub(crate) description_y: f64,
    pub(crate) divider_y: f64,
}

impl RectWithTitleGeometry {
    pub(crate) fn from_metrics(
        title_width: f64,
        title_height: f64,
        description_width: f64,
        description_height: f64,
        padding: f64,
    ) -> Self {
        let title_width = title_width.max(0.0);
        let title_height = title_height.max(0.0);
        let description_width = description_width.max(0.0);
        let description_height = description_height.max(0.0);
        let padding = padding.max(0.0);
        let half_padding = padding / 2.0;
        let label_width = title_width.max(description_width);
        let description_y = title_height + half_padding + 5.0;
        let label_height = description_y + description_height;

        Self {
            width: (label_width + padding).max(1.0),
            height: (label_height + padding).max(1.0),
            label_x: -label_width / 2.0,
            label_y: -label_height / 2.0 - half_padding + 3.0,
            title_x: ((label_width - title_width) / 2.0).max(0.0),
            description_x: ((label_width - description_width) / 2.0).max(0.0),
            description_y,
            divider_y: -label_height / 2.0 + title_height,
        }
    }
}

mod config;
#[cfg(feature = "layout-elk")]
mod elk;
mod layout;

pub(crate) use config::{StateConfigView, state_text_style};

pub(crate) use layout::layout_state_diagram_typed_with_work_meter;
pub use layout::{
    debug_build_state_diagram_dagre_graph, debug_extract_state_diagram_cluster_graph,
};

#[cfg(all(test, feature = "diagram-state"))]
mod tests {
    use super::RectWithTitleGeometry;

    #[test]
    fn rect_with_title_geometry_matches_upstream_equations_for_non_default_padding() {
        let geometry = RectWithTitleGeometry::from_metrics(80.0, 24.0, 60.0, 18.0, 12.0);

        assert_eq!(geometry.width, 92.0);
        assert_eq!(geometry.height, 65.0);
        assert_eq!(geometry.label_x, -40.0);
        assert_eq!(geometry.label_y, -29.5);
        assert_eq!(geometry.title_x, 0.0);
        assert_eq!(geometry.description_x, 10.0);
        assert_eq!(geometry.description_y, 35.0);
        assert_eq!(geometry.divider_y, -2.5);
    }
}
