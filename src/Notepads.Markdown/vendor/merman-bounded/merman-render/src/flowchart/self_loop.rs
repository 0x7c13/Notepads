use super::FlowEdge;
use merman_core::diagrams::flowchart::FlowEdgeMarker;

pub(crate) struct FlowchartSelfLoopHelperEdges {
    pub(crate) special_id_1: String,
    pub(crate) special_id_2: String,
    pub(crate) edge1: FlowEdge,
    pub(crate) edge_mid: FlowEdge,
    pub(crate) edge2: FlowEdge,
}

struct FlowchartSelfLoopEdgeSpec {
    id: String,
    from: String,
    to: String,
    label: Option<String>,
    label_type: Option<String>,
    edge_type: Option<String>,
    start_marker: FlowEdgeMarker,
    end_marker: FlowEdgeMarker,
}

pub(crate) fn flowchart_self_loop_helper_edges(base: &FlowEdge) -> FlowchartSelfLoopHelperEdges {
    let node_id = base.from.as_str();
    let special_id_1 = format!("{node_id}---{node_id}---1");
    let special_id_2 = format!("{node_id}---{node_id}---2");
    let endpoint_label = Some(String::new());

    let edge1 = flowchart_self_loop_edge_from_base(
        base,
        FlowchartSelfLoopEdgeSpec {
            id: format!("{node_id}-cyclic-special-1"),
            from: node_id.to_string(),
            to: special_id_1.clone(),
            label: endpoint_label.clone(),
            label_type: None,
            edge_type: Some("arrow_open".to_string()),
            start_marker: base.start_marker,
            end_marker: FlowEdgeMarker::None,
        },
    );
    let edge_mid = flowchart_self_loop_edge_from_base(
        base,
        FlowchartSelfLoopEdgeSpec {
            id: format!("{node_id}-cyclic-special-mid"),
            from: special_id_1.clone(),
            to: special_id_2.clone(),
            label: base.label.clone(),
            label_type: base.label_type.clone(),
            edge_type: Some("arrow_open".to_string()),
            start_marker: FlowEdgeMarker::None,
            end_marker: FlowEdgeMarker::None,
        },
    );
    let edge2 = flowchart_self_loop_edge_from_base(
        base,
        FlowchartSelfLoopEdgeSpec {
            id: format!("{node_id}-cyclic-special-2"),
            from: special_id_2.clone(),
            to: node_id.to_string(),
            label: endpoint_label,
            label_type: base.label_type.clone(),
            edge_type: base.edge_type.clone(),
            start_marker: FlowEdgeMarker::None,
            end_marker: base.end_marker,
        },
    );

    FlowchartSelfLoopHelperEdges {
        special_id_1,
        special_id_2,
        edge1,
        edge_mid,
        edge2,
    }
}

fn flowchart_self_loop_edge_from_base(
    base: &FlowEdge,
    spec: FlowchartSelfLoopEdgeSpec,
) -> FlowEdge {
    FlowEdge {
        id: spec.id,
        from: spec.from,
        to: spec.to,
        label: spec.label,
        label_type: spec.label_type,
        edge_type: spec.edge_type,
        arrow: base.arrow.clone(),
        start_marker: spec.start_marker,
        end_marker: spec.end_marker,
        is_user_defined_id: false,
        stroke: base.stroke.clone(),
        stroke_kind: base.stroke_kind,
        visibility: base.visibility,
        interpolate: base.interpolate.clone(),
        classes: base.classes.clone(),
        style: base.style.clone(),
        animate: base.animate,
        animation: base.animation.clone(),
        length: base.length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use merman_core::diagrams::flowchart::{FlowEdgeStroke, FlowEdgeVisibility};

    fn self_loop(start_marker: FlowEdgeMarker, end_marker: FlowEdgeMarker) -> FlowEdge {
        FlowEdge {
            id: "L_A_A_0".to_string(),
            from: "A".to_string(),
            to: "A".to_string(),
            label: Some("loop".to_string()),
            label_type: Some("text".to_string()),
            edge_type: Some("arrow_cross".to_string()),
            arrow: "o--x".to_string(),
            start_marker,
            end_marker,
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

    #[test]
    fn self_loop_helpers_keep_endpoint_markers_on_the_outer_segments_only() {
        for (start_marker, end_marker) in [
            (FlowEdgeMarker::Circle, FlowEdgeMarker::Cross),
            (FlowEdgeMarker::Point, FlowEdgeMarker::Point),
        ] {
            let helpers = flowchart_self_loop_helper_edges(&self_loop(start_marker, end_marker));

            assert_eq!(helpers.edge1.start_marker, start_marker);
            assert_eq!(helpers.edge1.end_marker, FlowEdgeMarker::None);
            assert_eq!(helpers.edge_mid.start_marker, FlowEdgeMarker::None);
            assert_eq!(helpers.edge_mid.end_marker, FlowEdgeMarker::None);
            assert_eq!(helpers.edge2.start_marker, FlowEdgeMarker::None);
            assert_eq!(helpers.edge2.end_marker, end_marker);
        }
    }
}
