use super::common::*;
use crate::Result;
use crate::swimlane::direction::LayoutWorkBudget;

const MINIMUM_FACE_CLEARANCE: f64 = 16.0;
const TRACK_SHIFT: f64 = 7.0;
const MAX_ITERATIONS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
struct TerminalLane {
    edge_index: usize,
    node_id: String,
    at_start: bool,
    orientation: Orientation,
    coordinate: f64,
    minimum: f64,
    maximum: f64,
    boundary: LayoutPoint,
    rail_end: LayoutPoint,
    rect: RectBounds,
}

fn orthogonally_aligned(first: &LayoutPoint, second: &LayoutPoint) -> bool {
    same_x(first, second, EPSILON) || same_y(first, second, EPSILON)
}

fn rect_intersection(node: &WorkingNode, point: &LayoutPoint) -> LayoutPoint {
    let delta_x = point.x - node.x;
    let delta_y = point.y - node.y;
    let mut half_width = node.width / 2.0;
    let mut half_height = node.height / 2.0;
    if delta_y.abs() * half_width > delta_x.abs() * half_height {
        if delta_y < 0.0 {
            half_height = -half_height;
        }
        return LayoutPoint {
            x: node.x
                + if delta_y == 0.0 {
                    0.0
                } else {
                    half_height * delta_x / delta_y
                },
            y: node.y + half_height,
        };
    }
    if delta_x < 0.0 {
        half_width = -half_width;
    }
    LayoutPoint {
        x: node.x + half_width,
        y: node.y
            + if delta_x == 0.0 {
                0.0
            } else {
                half_width * delta_y / delta_x
            },
    }
}

fn terminal_lane_for(
    edges: &[WorkingEdge],
    nodes: &indexmap::IndexMap<String, WorkingNode>,
    edge_index: usize,
    at_start: bool,
    replacement: Option<&[LayoutPoint]>,
) -> Option<TerminalLane> {
    let edge = &edges[edge_index];
    let points = dedupe_consecutive_points(replacement.unwrap_or(&edge.points), EPSILON);
    if points.len() < 2 {
        return None;
    }
    let node_id = if at_start { &edge.from } else { &edge.to };
    let node = nodes.get(node_id)?;
    let rect = rect_of_node_bounds(node)?;
    let endpoint = if at_start { &points[0] } else { points.last()? };
    let adjacent = if at_start {
        &points[1]
    } else {
        &points[points.len() - 2]
    };
    let boundary = rect_intersection(node, endpoint);
    let rail_end = if orthogonally_aligned(adjacent, &boundary) {
        adjacent.clone()
    } else {
        endpoint.clone()
    };

    if same_x(&boundary, &rail_end, EPSILON) {
        return Some(TerminalLane {
            edge_index,
            node_id: node_id.clone(),
            at_start,
            orientation: Orientation::Vertical,
            coordinate: boundary.x,
            minimum: boundary.y.min(rail_end.y),
            maximum: boundary.y.max(rail_end.y),
            boundary,
            rail_end,
            rect,
        });
    }
    if same_y(&boundary, &rail_end, EPSILON) {
        return Some(TerminalLane {
            edge_index,
            node_id: node_id.clone(),
            at_start,
            orientation: Orientation::Horizontal,
            coordinate: boundary.y,
            minimum: boundary.x.min(rail_end.x),
            maximum: boundary.x.max(rail_end.x),
            boundary,
            rail_end,
            rect,
        });
    }
    None
}

fn projected_overlap_length(first: &TerminalLane, second: &TerminalLane) -> f64 {
    (first.maximum.min(second.maximum) - first.minimum.max(second.minimum)).max(0.0)
}

fn same_terminal_face(first: &TerminalLane, second: &TerminalLane) -> bool {
    if first.node_id != second.node_id || first.orientation != second.orientation {
        return false;
    }
    match first.orientation {
        Orientation::Horizontal => {
            let on_horizontal_face = (first.boundary.x - first.rect.left).abs() < 1.0
                || (first.boundary.x - first.rect.right).abs() < 1.0;
            on_horizontal_face && same_x(&first.boundary, &second.boundary, 1.0)
        }
        Orientation::Vertical => {
            let on_vertical_face = (first.boundary.y - first.rect.top).abs() < 1.0
                || (first.boundary.y - first.rect.bottom).abs() < 1.0;
            on_vertical_face && same_y(&first.boundary, &second.boundary, 1.0)
        }
    }
}

fn exact_terminal_lane_conflict(first: &TerminalLane, second: &TerminalLane) -> bool {
    first.node_id == second.node_id
        && first.orientation == second.orientation
        && projected_overlap_length(first, second) >= MINIMUM_SHARED_LENGTH
        && (first.coordinate - second.coordinate).abs() < 0.5
}

fn near_terminal_lane_conflict(first: &TerminalLane, second: &TerminalLane) -> bool {
    if first.node_id != second.node_id
        || first.orientation != second.orientation
        || first.orientation != Orientation::Horizontal
        || first.at_start == second.at_start
    {
        return false;
    }
    let shared = projected_overlap_length(first, second);
    if shared < MINIMUM_SHARED_LENGTH {
        return false;
    }
    let face_span = first.rect.bottom - first.rect.top;
    shared >= face_span
        && shared <= 2.0 * face_span
        && same_terminal_face(first, second)
        && (first.coordinate - second.coordinate).abs() < MINIMUM_FACE_CLEARANCE
}

fn shifted_candidate(
    lane: &TerminalLane,
    edges: &[WorkingEdge],
    shift: f64,
) -> Option<Vec<LayoutPoint>> {
    let points = dedupe_consecutive_points(&edges[lane.edge_index].points, EPSILON);
    if points.len() < 2 {
        return None;
    }
    let shifted_boundary = match lane.orientation {
        Orientation::Vertical => LayoutPoint {
            x: lane.boundary.x + shift,
            y: lane.boundary.y,
        },
        Orientation::Horizontal => LayoutPoint {
            x: lane.boundary.x,
            y: lane.boundary.y + shift,
        },
    };
    let shifted_rail_end = match lane.orientation {
        Orientation::Vertical => LayoutPoint {
            x: lane.rail_end.x + shift,
            y: lane.rail_end.y,
        },
        Orientation::Horizontal => LayoutPoint {
            x: lane.rail_end.x,
            y: lane.rail_end.y + shift,
        },
    };

    let boundary_stays_on_same_face = if (lane.boundary.y - lane.rect.top).abs() < 1.0
        || (lane.boundary.y - lane.rect.bottom).abs() < 1.0
    {
        same_y(&shifted_boundary, &lane.boundary, EPSILON)
            && shifted_boundary.x >= lane.rect.left + 1.0
            && shifted_boundary.x <= lane.rect.right - 1.0
    } else if (lane.boundary.x - lane.rect.left).abs() < 1.0
        || (lane.boundary.x - lane.rect.right).abs() < 1.0
    {
        same_x(&shifted_boundary, &lane.boundary, EPSILON)
            && shifted_boundary.y >= lane.rect.top + 1.0
            && shifted_boundary.y <= lane.rect.bottom - 1.0
    } else {
        false
    };
    if !boundary_stays_on_same_face {
        return None;
    }

    if lane.at_start {
        let rail_end_is_adjacent = same_point(&points[1], &lane.rail_end, EPSILON);
        let rest_start = if rail_end_is_adjacent { 2 } else { 1 };
        let rest = &points[rest_start..];
        if rest
            .first()
            .is_some_and(|next| !orthogonally_aligned(next, &shifted_rail_end))
        {
            return None;
        }
        let mut candidate = vec![shifted_boundary, shifted_rail_end];
        candidate.extend_from_slice(rest);
        return Some(candidate);
    }

    let rail_end_is_adjacent = same_point(&points[points.len() - 2], &lane.rail_end, EPSILON);
    let before_end = if rail_end_is_adjacent {
        points.len() - 2
    } else {
        points.len() - 1
    };
    let before = &points[..before_end];
    if before
        .last()
        .is_some_and(|previous| !orthogonally_aligned(previous, &shifted_rail_end))
    {
        return None;
    }
    let mut candidate = before.to_vec();
    candidate.push(shifted_rail_end);
    candidate.push(shifted_boundary);
    Some(candidate)
}

fn lane_is_straight_collinear_connector(
    lane: &TerminalLane,
    edges: &[WorkingEdge],
    nodes: &indexmap::IndexMap<String, WorkingNode>,
) -> bool {
    let edge = &edges[lane.edge_index];
    let points = dedupe_consecutive_points(&edge.points, EPSILON);
    if points.len() != 2 {
        return false;
    }
    let (Some(start), Some(end)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else {
        return false;
    };
    let first = &points[0];
    let second = &points[1];
    (same_y(first, second, EPSILON)
        && (start.y - end.y).abs() < 1.0
        && (start.x - end.x).abs() > 1.0)
        || (same_x(first, second, EPSILON)
            && (start.x - end.x).abs() < 1.0
            && (start.y - end.y).abs() > 1.0)
}

pub(in crate::swimlane::direction) fn separate_shared_rendered_terminal_lanes(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    const SHIFTS: [f64; 6] = [
        -TRACK_SHIFT,
        TRACK_SHIFT,
        -2.0 * TRACK_SHIFT,
        2.0 * TRACK_SHIFT,
        -3.0 * TRACK_SHIFT,
        3.0 * TRACK_SHIFT,
    ];

    for _ in 0..MAX_ITERATIONS {
        work_budget.charge(layout.original_edges.len().saturating_mul(2))?;
        let lanes: Vec<_> = (0..layout.original_edges.len())
            .flat_map(|edge_index| {
                [
                    terminal_lane_for(
                        &layout.original_edges,
                        &layout.nodes,
                        edge_index,
                        true,
                        None,
                    ),
                    terminal_lane_for(
                        &layout.original_edges,
                        &layout.nodes,
                        edge_index,
                        false,
                        None,
                    ),
                ]
            })
            .flatten()
            .collect();

        let mut replacement = None;
        'pairs: for first_index in 0..lanes.len() {
            for second_index in first_index + 1..lanes.len() {
                work_budget.charge(1)?;
                let first = &lanes[first_index];
                let second = &lanes[second_index];
                let exact_conflict = exact_terminal_lane_conflict(first, second);
                if first.edge_index == second.edge_index
                    || (!exact_conflict && !near_terminal_lane_conflict(first, second))
                {
                    continue;
                }
                let fixing_near_conflict = !exact_conflict;
                let mut candidates = vec![first, second];
                candidates.sort_by_key(|lane| {
                    (
                        lane_is_straight_collinear_connector(
                            lane,
                            &layout.original_edges,
                            &layout.nodes,
                        ),
                        lane.at_start,
                    )
                });
                for lane in candidates {
                    for shift in SHIFTS {
                        work_budget.charge(layout.original_edges[lane.edge_index].points.len())?;
                        let Some(candidate) =
                            shifted_candidate(lane, &layout.original_edges, shift)
                        else {
                            continue;
                        };
                        work_budget.charge(1)?;
                        let Some(next_lane) = terminal_lane_for(
                            &layout.original_edges,
                            &layout.nodes,
                            lane.edge_index,
                            lane.at_start,
                            Some(&candidate),
                        ) else {
                            continue;
                        };
                        work_budget.charge(lanes.len().saturating_sub(1))?;
                        let conflicts = lanes.iter().any(|other| {
                            other.edge_index != lane.edge_index
                                && (exact_terminal_lane_conflict(&next_lane, other)
                                    || (fixing_near_conflict
                                        && near_terminal_lane_conflict(&next_lane, other)))
                        });
                        if !conflicts {
                            replacement = Some((lane.edge_index, candidate));
                            break 'pairs;
                        }
                    }
                }
            }
        }
        let Some((edge_index, points)) = replacement else {
            return Ok(());
        };
        layout.original_edges[edge_index].points = points;
    }
    Ok(())
}
