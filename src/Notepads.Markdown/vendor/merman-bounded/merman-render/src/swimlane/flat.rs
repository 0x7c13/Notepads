//! Adapter for already-measured flat diagrams using the shared Swimlane provider.

use super::config::{DEFAULT_LANE_ID, DEFAULT_LANE_PADDING, SwimlaneConfig};
use super::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
use crate::Result;
use crate::model::{LayoutEdge, LayoutNode, SwimlaneDirection, SwimlaneLayout};
use crate::resources::OperationWorkMeter;
use indexmap::IndexMap;
use merman_core::MermaidConfig;
use std::sync::Arc;

pub(crate) fn layout_flat(
    measured: &[(&LayoutNode, &str)],
    edges: &[LayoutEdge],
    config: &MermaidConfig,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<SwimlaneLayout> {
    work_meter.preflight(super::swimlane_layout_preflight_work_units(
        measured.len(),
        edges.len(),
    ))?;
    work_meter.charge(super::swimlane_core_layout_work_units(
        measured.len(),
        edges.len(),
    ))?;
    let mut nodes = IndexMap::new();
    for (node, shape) in measured {
        nodes.insert(
            node.id.clone(),
            WorkingNode {
                id: node.id.clone(),
                label: String::new(),
                label_type: "markdown".into(),
                shape: (*shape).to_owned(),
                kind: WorkingNodeKind::Content,
                parent_id: Some(DEFAULT_LANE_ID.into()),
                top_lane_id: Some(DEFAULT_LANE_ID.into()),
                requested_dir: None,
                padding: 0.0,
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height,
                label_width: node.label_width.unwrap_or(0.0),
                label_height: node.label_height.unwrap_or(0.0),
                layer: 0,
                order: 0,
                content_top: None,
                title_rect: None,
            },
        );
    }
    // prepareLayoutForSwimlanes places every loose node into one implicit lane.
    if !nodes.is_empty() {
        nodes.insert(
            DEFAULT_LANE_ID.into(),
            WorkingNode {
                id: DEFAULT_LANE_ID.into(),
                label: String::new(),
                label_type: "text".into(),
                shape: "swimlane".into(),
                kind: WorkingNodeKind::Group,
                parent_id: None,
                top_lane_id: None,
                requested_dir: Some("TB".into()),
                padding: DEFAULT_LANE_PADDING,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                label_width: 0.0,
                label_height: 0.0,
                layer: 0,
                order: 0,
                content_top: None,
                title_rect: None,
            },
        );
    }
    let original_edges: Vec<_> = edges
        .iter()
        .map(|edge| WorkingEdge {
            id: edge.id.clone(),
            from: edge.from.clone(),
            to: edge.to.clone(),
            reference_id: edge.id.clone(),
            label_node_id: None,
            reversed_for_layout: false,
            points: Vec::new(),
        })
        .collect();
    let mut working = WorkingLayout {
        direction: SwimlaneDirection::Tb,
        top_lane_order: if nodes.is_empty() {
            Vec::new()
        } else {
            vec![DEFAULT_LANE_ID.into()]
        },
        nodes,
        graph_edges: original_edges.clone(),
        original_edges,
    };
    super::run_layout_core(
        &mut working,
        SwimlaneConfig::from_config(config),
        work_meter,
    )?;
    Ok(super::project_layout(
        working,
        &std::collections::HashMap::new(),
    ))
}
