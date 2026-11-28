use super::common::*;
use crate::Result;
use crate::model::SwimlaneTitleRect;
use crate::swimlane::direction::LayoutWorkBudget;

const CLEARANCE: f64 = 4.0;

#[derive(Clone)]
struct LaneTitle {
    node_id: String,
    rect: RectBounds,
}

fn valid_title_rect(node: &WorkingNode) -> Option<RectBounds> {
    let rect = node.title_rect.as_ref()?;
    let values = [rect.left, rect.right, rect.top, rect.bottom];
    (values.into_iter().all(f64::is_finite) && rect.right > rect.left && rect.bottom > rect.top)
        .then_some(RectBounds {
            left: rect.left,
            right: rect.right,
            top: rect.top,
            bottom: rect.bottom,
        })
}

fn top_lane_title_for(node: &WorkingNode) -> Option<LaneTitle> {
    if node.kind != WorkingNodeKind::Group || node.parent_id.is_some() {
        return None;
    }
    let direction = node
        .requested_dir
        .as_deref()
        .unwrap_or_default()
        .to_uppercase();
    if matches!(direction.as_str(), "LR" | "RL" | "BT") {
        return None;
    }
    let rect = valid_title_rect(node)?;
    if !node.y.is_finite() || !node.height.is_finite() || node.height <= 0.0 {
        return None;
    }
    let title_width = rect.right - rect.left;
    let title_height = rect.bottom - rect.top;
    (title_height > 0.0 && title_width >= title_height).then(|| LaneTitle {
        node_id: node.id.clone(),
        rect,
    })
}

fn left_lane_title_for(node: &WorkingNode) -> Option<LaneTitle> {
    if node.kind != WorkingNodeKind::Group
        || node.parent_id.is_some()
        || node.requested_dir.as_deref() != Some("LR")
    {
        return None;
    }
    let rect = valid_title_rect(node)?;
    if !node.x.is_finite() || !node.width.is_finite() || node.width <= 0.0 {
        return None;
    }
    let title_width = rect.right - rect.left;
    let title_height = rect.bottom - rect.top;
    (title_width > 0.0 && title_height >= title_width).then(|| LaneTitle {
        node_id: node.id.clone(),
        rect,
    })
}

fn horizontal_segment_intersects_title(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    segment.horizontal
        && segment.a.y > rect.top + EPSILON
        && segment.a.y < rect.bottom - EPSILON
        && overlap_length(segment.a.x, segment.b.x, rect.left, rect.right) >= MINIMUM_SHARED_LENGTH
}

fn vertical_segment_intersects_title(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    segment.vertical
        && segment.a.x > rect.left + EPSILON
        && segment.a.x < rect.right - EPSILON
        && overlap_length(segment.a.y, segment.b.y, rect.top, rect.bottom) >= MINIMUM_SHARED_LENGTH
}

pub(in crate::swimlane::direction) fn lift_top_lane_title_bands_above_rails(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    let lanes: Vec<_> = layout
        .nodes
        .values()
        .filter_map(top_lane_title_for)
        .collect();
    if lanes.is_empty() {
        return Ok(());
    }

    let mut top_delta: f64 = 0.0;
    for edge in &layout.original_edges {
        let points = dedupe_consecutive_points(&edge.points, EPSILON);
        for segment in segments_for(&points) {
            work_budget.charge(lanes.len())?;
            for lane in &lanes {
                if horizontal_segment_intersects_title(&segment, lane.rect) {
                    top_delta = top_delta.max(lane.rect.bottom - segment.a.y + CLEARANCE);
                }
            }
        }
    }
    if top_delta <= EPSILON {
        return Ok(());
    }

    for lane in lanes {
        let Some(node) = layout.nodes.get_mut(&lane.node_id) else {
            continue;
        };
        if !node.y.is_finite() || !node.height.is_finite() || node.height <= 0.0 {
            continue;
        }
        node.y -= top_delta / 2.0;
        node.height += top_delta;
        node.title_rect = Some(SwimlaneTitleRect {
            left: lane.rect.left,
            right: lane.rect.right,
            top: lane.rect.top - top_delta,
            bottom: lane.rect.bottom - top_delta,
        });
    }
    Ok(())
}

pub(in crate::swimlane::direction) fn shift_left_lane_title_bands_left_of_rails(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    let lanes: Vec<_> = layout
        .nodes
        .values()
        .filter_map(left_lane_title_for)
        .collect();
    if lanes.is_empty() {
        return Ok(());
    }

    let mut left_delta: f64 = 0.0;
    for edge in &layout.original_edges {
        let points = dedupe_consecutive_points(&edge.points, EPSILON);
        for segment in segments_for(&points) {
            work_budget.charge(lanes.len())?;
            for lane in &lanes {
                if vertical_segment_intersects_title(&segment, lane.rect) {
                    left_delta = left_delta.max(lane.rect.right - segment.a.x + CLEARANCE);
                } else if horizontal_segment_intersects_title(&segment, lane.rect) {
                    let segment_left = segment.a.x.min(segment.b.x);
                    left_delta = left_delta.max(lane.rect.right - segment_left + CLEARANCE);
                }
            }
        }
    }
    if left_delta <= EPSILON {
        return Ok(());
    }

    for lane in lanes {
        let Some(node) = layout.nodes.get_mut(&lane.node_id) else {
            continue;
        };
        if !node.x.is_finite() || !node.width.is_finite() || node.width <= 0.0 {
            continue;
        }
        node.x -= left_delta / 2.0;
        node.width += left_delta;
        node.title_rect = Some(SwimlaneTitleRect {
            left: lane.rect.left - left_delta,
            right: lane.rect.right - left_delta,
            top: lane.rect.top,
            bottom: lane.rect.bottom,
        });
    }
    Ok(())
}
