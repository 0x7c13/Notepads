use merman_core::MermaidConfig;
use serde_json::Value;

use crate::config::{config_bool, value_at};

pub(super) const DEFAULT_LANE_ID: &str = "__swimlane_default__";
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(super) const GROUP_PADDING: f64 = 8.0;
pub(super) const DEFAULT_LANE_PADDING: f64 = 20.0;
pub(super) const TOP_LANE_TITLE_BAND_HEIGHT: f64 = 21.0;
pub(super) const MIN_TOP_LANE_HORIZONTAL_PADDING: f64 = 20.0;
pub(super) const TOP_LANE_MIN_HEADER_MARGIN: f64 = 36.0;
pub(super) const LR_TITLE_BAND_SIZE: f64 = 36.0;

pub(super) const ROUTER_NODE_PADDING: f64 = 8.0;
pub(super) const HORIZONTAL_PIPE_MARGIN: f64 = 15.0;
pub(super) const VERTICAL_PIPE_MARGIN: f64 = 15.0;
pub(super) const ROUTING_MARGIN: f64 = 25.0;
pub(super) const ANCHOR_OFFSET: f64 = 20.0;
pub(super) const VERTICAL_SIDE_BIAS: f64 = 3.0;
pub(super) const CROSSING_PENALTY: f64 = 1_000.0;
pub(super) const BEND_PENALTY: f64 = 50.0;
pub(super) const MIN_PORT_SPACING: f64 = 8.0;
pub(super) const MAX_PORT_SPACING: f64 = 20.0;
pub(super) const DIRECTION_SIGNIFICANCE: f64 = 10.0;
pub(super) const OPPOSITE_MOVE_THRESHOLD: f64 = 5.0;
pub(super) const WRONG_VERTICAL_DIRECTION_FACTOR: f64 = 100.0;
pub(super) const WRONG_HORIZONTAL_DIRECTION_FACTOR: f64 = 50.0;
pub(super) const EPSILON: f64 = 1.0e-6;

#[derive(Debug, Clone, Copy)]
pub(super) struct SwimlaneConfig {
    pub node_gap: f64,
    pub layer_gap: f64,
    pub ignore_cross_lane_edges: bool,
    pub optimize_ranks_by_crossings: bool,
    pub automatic_lane_ordering: bool,
}

fn number_at(root: &Value, path: &[&str]) -> Option<f64> {
    let value = value_at(root, path)?;
    value.as_f64().or_else(|| {
        value
            .as_str()
            .and_then(|text| text.trim().parse::<f64>().ok())
    })
}

impl SwimlaneConfig {
    pub fn from_config(config: &MermaidConfig) -> Self {
        let root = config.as_value();
        Self {
            // Mermaid layoutCore uses nullish-coalescing here. In a fully merged config the
            // normal values are Flowchart's 50/50 defaults; a user-provided zero remains zero.
            node_gap: number_at(root, &["flowchart", "nodeSpacing"]).unwrap_or(40.0),
            layer_gap: number_at(root, &["flowchart", "rankSpacing"]).unwrap_or(100.0),
            ignore_cross_lane_edges: config_bool(root, &["swimlane", "ignoreCrossLaneEdges"])
                .unwrap_or(true),
            optimize_ranks_by_crossings: config_bool(
                root,
                &["swimlane", "optimizeRanksByCrossings"],
            )
            .unwrap_or(true),
            automatic_lane_ordering: config_bool(root, &["swimlane", "automaticLaneOrdering"])
                .unwrap_or(false),
        }
    }
}
