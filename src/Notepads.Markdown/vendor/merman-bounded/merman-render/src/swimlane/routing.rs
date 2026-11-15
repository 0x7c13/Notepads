use super::config::{
    ANCHOR_OFFSET, BEND_PENALTY, CROSSING_PENALTY, DIRECTION_SIGNIFICANCE, EPSILON,
    HORIZONTAL_PIPE_MARGIN, MAX_PORT_SPACING, MIN_PORT_SPACING, OPPOSITE_MOVE_THRESHOLD,
    ROUTER_NODE_PADDING, ROUTING_MARGIN, VERTICAL_PIPE_MARGIN, VERTICAL_SIDE_BIAS,
    WRONG_HORIZONTAL_DIRECTION_FACTOR, WRONG_VERTICAL_DIRECTION_FACTOR,
};
use super::direction::geometry::{
    EPSILON as GEOMETRY_EPSILON, orthogonal_segments_strictly_cross, simplify_polyline,
};
use super::geometry::{Rect, segment_blocked};
use super::work_budget::{LayoutWorkBudget, sorting_work_units};
use super::working::{WorkingLayout, WorkingNodeKind};
use crate::Result;
use crate::model::LayoutPoint;
use indexmap::IndexMap;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[path = "routing/tracks.rs"]
mod tracks;

// Mermaid's orthogonal router leaves this much room between an endpoint handle
// and the obstacle boundary before turning onto the detour pipe.
const HANDLE_CLEARANCE: f64 = 2.0;

#[derive(Debug, Clone)]
struct NodeGeometry {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    shape: String,
    top_lane_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone)]
struct EdgeSideInfo {
    edge_index: usize,
    source_id: String,
    target_id: String,
    source_side: Side,
    target_side: Side,
    abs_dx: f64,
    abs_dy: f64,
    dx_sign: f64,
    dy_sign: f64,
}

fn preference_strength(info: &EdgeSideInfo) -> f64 {
    if matches!(info.source_side, Side::Top | Side::Bottom) {
        if info.abs_dx == 0.0 {
            f64::INFINITY
        } else {
            info.abs_dy / info.abs_dx
        }
    } else if info.abs_dy == 0.0 {
        f64::INFINITY
    } else {
        info.abs_dx / info.abs_dy
    }
}

fn secondary_side(info: &EdgeSideInfo) -> Side {
    if matches!(info.source_side, Side::Top | Side::Bottom) {
        if info.dx_sign >= 0.0 {
            Side::Right
        } else {
            Side::Left
        }
    } else if info.dy_sign >= 0.0 {
        Side::Bottom
    } else {
        Side::Top
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Axis {
    None,
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct QueueState {
    estimate: f64,
    cost: f64,
    x_index: usize,
    y_index: usize,
    axis: Axis,
}

impl PartialEq for QueueState {
    fn eq(&self, other: &Self) -> bool {
        self.estimate.to_bits() == other.estimate.to_bits()
            && self.cost.to_bits() == other.cost.to_bits()
            && self.x_index == other.x_index
            && self.y_index == other.y_index
            && self.axis == other.axis
    }
}

impl Eq for QueueState {}

impl PartialOrd for QueueState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.cost.total_cmp(&self.cost))
            .then_with(|| other.y_index.cmp(&self.y_index))
            .then_with(|| other.x_index.cmp(&self.x_index))
    }
}

fn choose_side(node: &NodeGeometry, target: &LayoutPoint, fallback: Side) -> Side {
    let dx = target.x - node.x;
    let dy = target.y - node.y;
    let abs_dx = dx.abs();
    let abs_dy = dy.abs();
    if abs_dx < EPSILON && abs_dy < EPSILON {
        return fallback;
    }
    if abs_dy > EPSILON && abs_dy * VERTICAL_SIDE_BIAS >= abs_dx {
        if dy > 0.0 { Side::Bottom } else { Side::Top }
    } else if abs_dx > EPSILON {
        if dx > 0.0 { Side::Right } else { Side::Left }
    } else {
        fallback
    }
}

fn port_for_side(node: &NodeGeometry, side: Side) -> LayoutPoint {
    match side {
        Side::Top => LayoutPoint {
            x: node.x,
            y: node.y - node.height / 2.0,
        },
        Side::Bottom => LayoutPoint {
            x: node.x,
            y: node.y + node.height / 2.0,
        },
        Side::Left => LayoutPoint {
            x: node.x - node.width / 2.0,
            y: node.y,
        },
        Side::Right => LayoutPoint {
            x: node.x + node.width / 2.0,
            y: node.y,
        },
    }
}

fn offset_port(mut point: LayoutPoint, side: Side, offset: f64) -> LayoutPoint {
    match side {
        Side::Top | Side::Bottom => point.x += offset,
        Side::Left | Side::Right => point.y += offset,
    }
    point
}

fn anchor_for_port(port: &LayoutPoint, node: &NodeGeometry, side: Side) -> LayoutPoint {
    match side {
        Side::Top => LayoutPoint {
            x: port.x,
            y: node.y - node.height / 2.0 - ANCHOR_OFFSET,
        },
        Side::Bottom => LayoutPoint {
            x: port.x,
            y: node.y + node.height / 2.0 + ANCHOR_OFFSET,
        },
        Side::Left => LayoutPoint {
            x: node.x - node.width / 2.0 - ANCHOR_OFFSET,
            y: port.y,
        },
        Side::Right => LayoutPoint {
            x: node.x + node.width / 2.0 + ANCHOR_OFFSET,
            y: port.y,
        },
    }
}

#[derive(Debug)]
struct AnchorHandle {
    anchor: LayoutPoint,
    waypoints_from_port: Vec<LayoutPoint>,
}

fn buried_anchor_handle(
    port: &LayoutPoint,
    anchor: &LayoutPoint,
    opposite: &NodeGeometry,
    side: Side,
    obstacles: &[(String, Rect)],
    excluded: &[&str],
) -> Option<AnchorHandle> {
    let obstacle = obstacles
        .iter()
        .find(|(id, rect)| !excluded.contains(&id.as_str()) && rect.contains_point(anchor, 0.0))?
        .1;

    if matches!(side, Side::Top | Side::Bottom) {
        let leaves_positive_side = side == Side::Bottom;
        let detour_x = if opposite.x >= port.x {
            obstacle.right + HORIZONTAL_PIPE_MARGIN
        } else {
            obstacle.left - HORIZONTAL_PIPE_MARGIN
        };
        let detour_y = if leaves_positive_side {
            obstacle.bottom + VERTICAL_PIPE_MARGIN
        } else {
            obstacle.top - VERTICAL_PIPE_MARGIN
        };
        let gap_y = if leaves_positive_side {
            (obstacle.top - HANDLE_CLEARANCE).min(port.y + ANCHOR_OFFSET)
        } else {
            (obstacle.bottom + HANDLE_CLEARANCE).max(port.y - ANCHOR_OFFSET)
        };
        let anchor = LayoutPoint {
            x: detour_x,
            y: detour_y,
        };
        return Some(AnchorHandle {
            anchor: anchor.clone(),
            waypoints_from_port: vec![
                LayoutPoint {
                    x: port.x,
                    y: gap_y,
                },
                LayoutPoint {
                    x: detour_x,
                    y: gap_y,
                },
                anchor,
            ],
        });
    }

    let leaves_positive_side = side == Side::Right;
    let detour_x = if leaves_positive_side {
        obstacle.right + HORIZONTAL_PIPE_MARGIN
    } else {
        obstacle.left - HORIZONTAL_PIPE_MARGIN
    };
    let detour_y = if opposite.y >= port.y {
        obstacle.bottom + VERTICAL_PIPE_MARGIN
    } else {
        obstacle.top - VERTICAL_PIPE_MARGIN
    };
    let gap_x = if leaves_positive_side {
        (obstacle.left - HANDLE_CLEARANCE).min(port.x + ANCHOR_OFFSET)
    } else {
        (obstacle.right + HANDLE_CLEARANCE).max(port.x - ANCHOR_OFFSET)
    };
    let anchor = LayoutPoint {
        x: detour_x,
        y: detour_y,
    };
    Some(AnchorHandle {
        anchor: anchor.clone(),
        waypoints_from_port: vec![
            LayoutPoint {
                x: gap_x,
                y: port.y,
            },
            LayoutPoint {
                x: gap_x,
                y: detour_y,
            },
            anchor,
        ],
    })
}

fn crossing_cost(from: &LayoutPoint, to: &LayoutPoint, routed: &[Vec<LayoutPoint>]) -> f64 {
    routed
        .iter()
        .flat_map(|points| points.windows(2))
        .filter(|segment| {
            orthogonal_segments_strictly_cross(from, to, &segment[0], &segment[1], GEOMETRY_EPSILON)
        })
        .count() as f64
        * CROSSING_PENALTY
}

fn direct_path(
    start: &LayoutPoint,
    end: &LayoutPoint,
    obstacles: &[(String, Rect)],
    excluded: &[&str],
) -> Option<Vec<LayoutPoint>> {
    let horizontal_first = LayoutPoint {
        x: end.x,
        y: start.y,
    };
    if !segment_blocked(start, &horizontal_first, obstacles, excluded)
        && !segment_blocked(&horizontal_first, end, obstacles, excluded)
    {
        return Some(simplify_polyline(&[
            start.clone(),
            horizontal_first,
            end.clone(),
        ]));
    }
    let vertical_first = LayoutPoint {
        x: start.x,
        y: end.y,
    };
    if !segment_blocked(start, &vertical_first, obstacles, excluded)
        && !segment_blocked(&vertical_first, end, obstacles, excluded)
    {
        return Some(simplify_polyline(&[
            start.clone(),
            vertical_first,
            end.clone(),
        ]));
    }
    None
}

fn coordinate_index(values: &[f64], target: f64) -> Option<usize> {
    values
        .iter()
        .position(|value| (*value - target).abs() <= EPSILON)
}

fn state_key(x_index: usize, y_index: usize, axis: Axis) -> (usize, usize, Axis) {
    (x_index, y_index, axis)
}

fn a_star_grid_upper_bound(obstacle_count: usize) -> usize {
    let axis_coordinates = obstacle_count.saturating_mul(2).saturating_add(4);
    axis_coordinates.saturating_mul(axis_coordinates)
}

fn a_star_grid_setup_work_units(obstacle_count: usize) -> usize {
    let axis_coordinates = obstacle_count.saturating_mul(2).saturating_add(4);
    axis_coordinates
        .saturating_mul(2)
        .saturating_add(sorting_work_units(axis_coordinates).saturating_mul(2))
}

fn a_star_preflight_work_units(obstacle_count: usize) -> usize {
    a_star_grid_upper_bound(obstacle_count)
        .saturating_add(a_star_grid_setup_work_units(obstacle_count))
}

fn a_star_path(
    start: &LayoutPoint,
    end: &LayoutPoint,
    obstacles: &[(String, Rect)],
    excluded: &[&str],
    routed: &[Vec<LayoutPoint>],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<Vec<LayoutPoint>>> {
    let obstacle_count = obstacles
        .iter()
        .filter(|(id, _)| !excluded.contains(&id.as_str()))
        .count();
    work_budget.preflight(a_star_preflight_work_units(obstacle_count))?;
    let routed_segment_count = routed
        .iter()
        .map(|points| points.len().saturating_sub(1))
        .fold(0usize, usize::saturating_add);

    // Charge coordinate storage and both stable sorts before allocating either axis vector.
    work_budget.charge(a_star_grid_setup_work_units(obstacle_count))?;
    let mut x_values = vec![start.x, end.x];
    let mut y_values = vec![start.y, end.y];
    for (id, obstacle) in obstacles {
        if excluded.contains(&id.as_str()) {
            continue;
        }
        x_values.extend([
            obstacle.left - VERTICAL_PIPE_MARGIN,
            obstacle.right + VERTICAL_PIPE_MARGIN,
        ]);
        y_values.extend([
            obstacle.top - HORIZONTAL_PIPE_MARGIN,
            obstacle.bottom + HORIZONTAL_PIPE_MARGIN,
        ]);
    }
    let min_x = x_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_x = x_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_y = y_values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_y = y_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    x_values.extend([min_x - ROUTING_MARGIN, max_x + ROUTING_MARGIN]);
    y_values.extend([min_y - ROUTING_MARGIN, max_y + ROUTING_MARGIN]);
    x_values.sort_by(f64::total_cmp);
    y_values.sort_by(f64::total_cmp);
    x_values.dedup_by(|left, right| (*left - *right).abs() <= EPSILON);
    y_values.dedup_by(|left, right| (*left - *right).abs() <= EPSILON);

    let Some(start_x) = coordinate_index(&x_values, start.x) else {
        return Ok(None);
    };
    let Some(start_y) = coordinate_index(&y_values, start.y) else {
        return Ok(None);
    };
    let Some(end_x) = coordinate_index(&x_values, end.x) else {
        return Ok(None);
    };
    let Some(end_y) = coordinate_index(&y_values, end.y) else {
        return Ok(None);
    };
    let start_key = state_key(start_x, start_y, Axis::None);
    let mut open = BinaryHeap::new();
    open.push(QueueState {
        estimate: (start.x - end.x).abs() + (start.y - end.y).abs(),
        cost: 0.0,
        x_index: start_x,
        y_index: start_y,
        axis: Axis::None,
    });
    let mut score = HashMap::new();
    score.insert(start_key, 0.0);
    let mut previous: HashMap<(usize, usize, Axis), (usize, usize, Axis)> = HashMap::new();
    let mut goal = None;

    while let Some(current) = open.pop() {
        work_budget.charge(1)?;
        let key = state_key(current.x_index, current.y_index, current.axis);
        if current.cost > score.get(&key).copied().unwrap_or(f64::INFINITY) + EPSILON {
            continue;
        }
        if current.x_index == end_x && current.y_index == end_y {
            goal = Some(key);
            break;
        }
        let current_point = LayoutPoint {
            x: x_values[current.x_index],
            y: y_values[current.y_index],
        };
        let mut neighbors = Vec::with_capacity(4);
        if current.x_index > 0 {
            neighbors.push((current.x_index - 1, current.y_index, Axis::Horizontal));
        }
        if current.x_index + 1 < x_values.len() {
            neighbors.push((current.x_index + 1, current.y_index, Axis::Horizontal));
        }
        if current.y_index > 0 {
            neighbors.push((current.x_index, current.y_index - 1, Axis::Vertical));
        }
        if current.y_index + 1 < y_values.len() {
            neighbors.push((current.x_index, current.y_index + 1, Axis::Vertical));
        }

        for (next_x, next_y, next_axis) in neighbors {
            let next_point = LayoutPoint {
                x: x_values[next_x],
                y: y_values[next_y],
            };
            work_budget.charge(obstacle_count.saturating_mul(2))?;
            if obstacles.iter().any(|(id, rect)| {
                !excluded.contains(&id.as_str()) && rect.contains_point(&next_point, 0.0)
            }) || segment_blocked(&current_point, &next_point, obstacles, excluded)
            {
                continue;
            }
            let move_x = next_point.x - current_point.x;
            let move_y = next_point.y - current_point.y;
            let destination_x = end.x - start.x;
            let destination_y = end.y - start.y;
            let mut direction_penalty = 0.0;
            if (destination_y > DIRECTION_SIGNIFICANCE && move_y < -OPPOSITE_MOVE_THRESHOLD)
                || (destination_y < -DIRECTION_SIGNIFICANCE && move_y > OPPOSITE_MOVE_THRESHOLD)
            {
                direction_penalty += move_y.abs() * WRONG_VERTICAL_DIRECTION_FACTOR;
            }
            if (destination_x > DIRECTION_SIGNIFICANCE && move_x < -OPPOSITE_MOVE_THRESHOLD)
                || (destination_x < -DIRECTION_SIGNIFICANCE && move_x > OPPOSITE_MOVE_THRESHOLD)
            {
                direction_penalty += move_x.abs() * WRONG_HORIZONTAL_DIRECTION_FACTOR;
            }
            let bend_penalty = if current.axis != Axis::None && current.axis != next_axis {
                BEND_PENALTY
            } else {
                0.0
            };
            let distance = move_x.abs() + move_y.abs();
            work_budget.charge(routed_segment_count)?;
            let next_cost = current.cost
                + distance
                + bend_penalty
                + direction_penalty
                + crossing_cost(&current_point, &next_point, routed);
            let next_key = state_key(next_x, next_y, next_axis);
            if next_cost + EPSILON >= score.get(&next_key).copied().unwrap_or(f64::INFINITY) {
                continue;
            }
            score.insert(next_key, next_cost);
            previous.insert(next_key, key);
            let heuristic = (next_point.x - end.x).abs() + (next_point.y - end.y).abs();
            open.push(QueueState {
                estimate: next_cost + heuristic,
                cost: next_cost,
                x_index: next_x,
                y_index: next_y,
                axis: next_axis,
            });
        }
    }

    let Some(mut current) = goal else {
        return Ok(None);
    };
    let mut reversed = Vec::new();
    loop {
        reversed.push(LayoutPoint {
            x: x_values[current.0],
            y: y_values[current.1],
        });
        if current == start_key {
            break;
        }
        let Some(previous) = previous.get(&current) else {
            return Ok(None);
        };
        current = *previous;
    }
    reversed.reverse();
    Ok(Some(simplify_polyline(&reversed)))
}

fn self_loop_route(node: &NodeGeometry) -> Vec<LayoutPoint> {
    let right = node.x + node.width / 2.0;
    let top = node.y - node.height / 2.0;
    let rail_x = right + ROUTING_MARGIN;
    let rail_y = top - ROUTING_MARGIN;
    vec![
        LayoutPoint {
            x: right,
            y: node.y,
        },
        LayoutPoint {
            x: rail_x,
            y: node.y,
        },
        LayoutPoint {
            x: rail_x,
            y: rail_y,
        },
        LayoutPoint {
            x: node.x,
            y: rail_y,
        },
        LayoutPoint { x: node.x, y: top },
    ]
}

pub(super) fn route(layout: &mut WorkingLayout, work_budget: &mut LayoutWorkBudget) -> Result<()> {
    work_budget.charge(
        layout
            .nodes
            .len()
            .saturating_add(layout.original_edges.len()),
    )?;
    let geometries: HashMap<String, NodeGeometry> = layout
        .nodes
        .values()
        .filter(|node| node.kind != WorkingNodeKind::Dummy)
        .map(|node| {
            (
                node.id.clone(),
                NodeGeometry {
                    x: node.x,
                    y: node.y,
                    width: node.width,
                    height: node.height,
                    shape: node.shape.clone(),
                    top_lane_id: node.top_lane_id.clone(),
                },
            )
        })
        .collect();
    let obstacles: Vec<(String, Rect)> = layout
        .nodes
        .values()
        .filter(|node| node.kind == WorkingNodeKind::Content)
        .map(|node| {
            (
                node.id.clone(),
                Rect::from_center(node.x, node.y, node.width, node.height)
                    .inflated(ROUTER_NODE_PADDING),
            )
        })
        .collect();

    let mut side_info = vec![None; layout.original_edges.len()];
    for (index, edge) in layout.original_edges.iter().enumerate() {
        let (Some(source), Some(target)) = (geometries.get(&edge.from), geometries.get(&edge.to))
        else {
            continue;
        };
        if edge.from == edge.to {
            continue;
        }
        let dx = target.x - source.x;
        let dy = target.y - source.y;
        side_info[index] = Some(EdgeSideInfo {
            edge_index: index,
            source_id: edge.from.clone(),
            target_id: edge.to.clone(),
            source_side: choose_side(
                source,
                &LayoutPoint {
                    x: target.x,
                    y: target.y,
                },
                Side::Bottom,
            ),
            target_side: choose_side(
                target,
                &LayoutPoint {
                    x: source.x,
                    y: source.y,
                },
                Side::Bottom,
            ),
            abs_dx: dx.abs(),
            abs_dy: dy.abs(),
            dx_sign: dx.signum(),
            dy_sign: dy.signum(),
        });
    }

    // Raykov Step 6.2 (diss.pdf section 6.1.2.2): preserve the strongest
    // preferred source side and move weaker siblings to a less-loaded secondary side.
    let mut source_side_groups: IndexMap<(String, Side), Vec<usize>> = IndexMap::new();
    let mut side_load: HashMap<(String, Side), usize> = HashMap::new();
    for info in side_info.iter().flatten() {
        source_side_groups
            .entry((info.source_id.clone(), info.source_side))
            .or_default()
            .push(info.edge_index);
        *side_load
            .entry((info.source_id.clone(), info.source_side))
            .or_default() += 1;
        *side_load
            .entry((info.target_id.clone(), info.target_side))
            .or_default() += 1;
    }
    for group in source_side_groups.values_mut() {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|left, right| {
            let left_strength = preference_strength(side_info[*left].as_ref().expect("side info"));
            let right_strength =
                preference_strength(side_info[*right].as_ref().expect("side info"));
            right_strength
                .total_cmp(&left_strength)
                .then_with(|| left.cmp(right))
        });
        for edge_index in group.iter().skip(1).copied() {
            let info = side_info[edge_index].as_ref().expect("side info");
            let secondary = secondary_side(info);
            let primary_key = (info.source_id.clone(), info.source_side);
            let secondary_key = (info.source_id.clone(), secondary);
            let primary_load = side_load.get(&primary_key).copied().unwrap_or(0);
            let secondary_load = side_load.get(&secondary_key).copied().unwrap_or(0);
            if secondary_load >= primary_load {
                continue;
            }
            side_load.insert(primary_key, primary_load.saturating_sub(1));
            side_load.insert(secondary_key, secondary_load + 1);
            side_info[edge_index]
                .as_mut()
                .expect("side info")
                .source_side = secondary;
        }
    }

    // Step 6.2b: diamond nodes need bimodal in/out side intervals. Their pointy
    // faces cannot safely host an incoming and outgoing connector on one pin.
    let mut incoming_sides: HashMap<String, std::collections::HashSet<Side>> = HashMap::new();
    for info in side_info.iter().flatten() {
        incoming_sides
            .entry(info.target_id.clone())
            .or_default()
            .insert(info.target_side);
    }
    for info in side_info.iter_mut().flatten() {
        let Some(source) = geometries.get(&info.source_id) else {
            continue;
        };
        if !matches!(source.shape.as_str(), "question" | "diamond") {
            continue;
        }
        let Some(used_by_incoming) = incoming_sides.get(&info.source_id) else {
            continue;
        };
        if !used_by_incoming.contains(&info.source_side) {
            continue;
        }
        let secondary = secondary_side(info);
        if used_by_incoming.contains(&secondary)
            || side_load
                .get(&(info.source_id.clone(), secondary))
                .copied()
                .unwrap_or(0)
                > 0
        {
            continue;
        }
        let primary_key = (info.source_id.clone(), info.source_side);
        let secondary_key = (info.source_id.clone(), secondary);
        let primary_load = side_load.get(&primary_key).copied().unwrap_or(0);
        side_load.insert(primary_key, primary_load.saturating_sub(1));
        side_load.insert(secondary_key, 1);
        info.source_side = secondary;
    }

    let mut port_groups: IndexMap<(String, Side, bool), Vec<(usize, f64)>> = IndexMap::new();
    for info in side_info.iter().flatten() {
        let source = &geometries[&info.source_id];
        let target = &geometries[&info.target_id];
        let source_opposite = match info.source_side {
            Side::Top | Side::Bottom => target.x,
            Side::Left | Side::Right => target.y,
        };
        let target_opposite = match info.target_side {
            Side::Top | Side::Bottom => source.x,
            Side::Left | Side::Right => source.y,
        };
        port_groups
            .entry((info.source_id.clone(), info.source_side, true))
            .or_default()
            .push((info.edge_index, source_opposite));
        port_groups
            .entry((info.target_id.clone(), info.target_side, false))
            .or_default()
            .push((info.edge_index, target_opposite));
    }

    let mut offsets: HashMap<(usize, bool), f64> = HashMap::new();
    for ((node_id, side, source_role), group) in &mut port_groups {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|left, right| left.1.total_cmp(&right.1));
        let Some(node) = geometries.get(node_id) else {
            continue;
        };
        let side_length = match side {
            Side::Left | Side::Right => node.height,
            Side::Top | Side::Bottom => node.width,
        };
        let effective_length = if matches!(node.shape.as_str(), "question" | "diamond") {
            side_length * 0.3
        } else {
            side_length
        };
        let spacing = effective_length / (group.len() + 1) as f64;
        let spacing = if spacing.is_nan() {
            MIN_PORT_SPACING
        } else {
            spacing.clamp(MIN_PORT_SPACING, MAX_PORT_SPACING)
        };
        let start = -spacing * group.len().saturating_sub(1) as f64 / 2.0;
        for (position, (edge_index, _)) in group.iter().enumerate() {
            offsets.insert(
                (*edge_index, *source_role),
                start + position as f64 * spacing,
            );
        }
    }

    let mut routing_order: Vec<usize> = (0..layout.original_edges.len()).collect();
    routing_order.sort_by(|left, right| {
        let score = |index: usize| {
            let edge = &layout.original_edges[index];
            let source = geometries.get(&edge.from);
            let target = geometries.get(&edge.to);
            let cross_lane = source
                .zip(target)
                .is_some_and(|(source, target)| source.top_lane_id != target.top_lane_id);
            let distance = source.zip(target).map_or(0.0, |(source, target)| {
                (source.x - target.x).abs() + (source.y - target.y).abs()
            });
            (cross_lane, distance)
        };
        let (left_cross, left_distance) = score(*left);
        let (right_cross, right_distance) = score(*right);
        right_cross
            .cmp(&left_cross)
            .then_with(|| left_distance.total_cmp(&right_distance))
            .then_with(|| left.cmp(right))
    });

    let mut routed = Vec::new();
    let mut results: HashMap<usize, Vec<LayoutPoint>> = HashMap::new();
    let mut centered_straight_edges = HashSet::new();
    for &index in &routing_order {
        let edge = &layout.original_edges[index];
        let (Some(source), Some(target)) = (geometries.get(&edge.from), geometries.get(&edge.to))
        else {
            continue;
        };
        if edge.from == edge.to {
            let points = self_loop_route(source);
            routed.push(points.clone());
            results.insert(index, points);
            continue;
        }
        let Some(info) = &side_info[index] else {
            continue;
        };
        let source_side = info.source_side;
        let target_side = info.target_side;
        let source_port = offset_port(
            port_for_side(source, source_side),
            source_side,
            offsets.get(&(index, true)).copied().unwrap_or(0.0),
        );
        let target_port = offset_port(
            port_for_side(target, target_side),
            target_side,
            offsets.get(&(index, false)).copied().unwrap_or(0.0),
        );
        let mut source_anchor = anchor_for_port(&source_port, source, source_side);
        let mut target_anchor = anchor_for_port(&target_port, target, target_side);
        let excluded = [edge.from.as_str(), edge.to.as_str()];
        work_budget.charge(obstacles.len().saturating_mul(6))?;
        let source_handle = buried_anchor_handle(
            &source_port,
            &source_anchor,
            target,
            source_side,
            &obstacles,
            &excluded,
        );
        if let Some(handle) = &source_handle {
            source_anchor = handle.anchor.clone();
        }
        let target_handle = buried_anchor_handle(
            &target_port,
            &target_anchor,
            source,
            target_side,
            &obstacles,
            &excluded,
        );
        if let Some(handle) = &target_handle {
            target_anchor = handle.anchor.clone();
        }
        let middle = if let Some(path) =
            direct_path(&source_anchor, &target_anchor, &obstacles, &excluded)
        {
            path
        } else if let Some(path) = a_star_path(
            &source_anchor,
            &target_anchor,
            &obstacles,
            &excluded,
            &routed,
            work_budget,
        )? {
            path
        } else {
            vec![
                source_anchor.clone(),
                LayoutPoint {
                    x: source_anchor.x,
                    y: target_anchor.y,
                },
                target_anchor.clone(),
            ]
        };
        let handle_capacity = source_handle
            .as_ref()
            .map_or(0, |handle| handle.waypoints_from_port.len())
            + target_handle
                .as_ref()
                .map_or(0, |handle| handle.waypoints_from_port.len());
        let mut points = Vec::with_capacity(middle.len() + handle_capacity + 2);
        points.push(source_port);
        if let Some(handle) = &source_handle {
            points.extend(handle.waypoints_from_port.iter().cloned());
            points.extend(middle.iter().skip(1).cloned());
        } else {
            points.extend(middle);
        }
        if let Some(handle) = &target_handle {
            points.extend(handle.waypoints_from_port.iter().rev().skip(1).cloned());
        }
        points.push(target_port);
        let points = simplify_polyline(&points);
        if points.len() == 2
            && !offsets.contains_key(&(index, true))
            && !offsets.contains_key(&(index, false))
            && ((points[0].x - points[1].x).abs() <= EPSILON
                || (points[0].y - points[1].y).abs() <= EPSILON)
        {
            centered_straight_edges.insert(index);
        }
        routed.push(points.clone());
        results.insert(index, points);
    }

    let staging_work = layout
        .original_edges
        .iter()
        .fold(layout.original_edges.len(), |work, edge| {
            work.saturating_add(edge.points.len())
        });
    work_budget.charge(staging_work)?;
    let mut staged_edges = layout.original_edges.clone();
    for (index, points) in results {
        staged_edges[index].points = points;
    }
    tracks::assign_tracks(
        &mut staged_edges,
        &routing_order,
        &centered_straight_edges,
        work_budget,
    )?;
    layout.original_edges = staged_edges;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SwimlaneDirection;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};
    use crate::swimlane::working::{WorkingEdge, WorkingNode};
    use indexmap::IndexMap;

    fn node(id: &str, x: f64, y: f64, width: f64, height: f64) -> WorkingNode {
        WorkingNode {
            id: id.to_string(),
            label: id.to_string(),
            label_type: "text".to_string(),
            shape: "rect".to_string(),
            kind: WorkingNodeKind::Content,
            parent_id: None,
            top_lane_id: None,
            requested_dir: None,
            padding: 0.0,
            x,
            y,
            width,
            height,
            label_width: width,
            label_height: height,
            layer: 0,
            order: 0,
            content_top: None,
            title_rect: None,
        }
    }

    fn edge(id: &str, from: &str, to: &str) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            reference_id: id.to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points: Vec::new(),
        }
    }

    fn layout(nodes: Vec<WorkingNode>, edges: Vec<WorkingEdge>) -> WorkingLayout {
        WorkingLayout {
            direction: SwimlaneDirection::Tb,
            nodes: nodes
                .into_iter()
                .map(|node| (node.id.clone(), node))
                .collect::<IndexMap<_, _>>(),
            graph_edges: edges.clone(),
            original_edges: edges,
            top_lane_order: Vec::new(),
        }
    }

    fn route_unbounded(layout: &mut WorkingLayout) {
        route(layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .expect("unbounded routing must succeed");
    }

    #[test]
    fn a_star_grid_is_rejected_before_search_allocation() {
        let obstacles = (0..32)
            .map(|index| {
                (
                    format!("obstacle-{index}"),
                    Rect {
                        left: index as f64 * 2.0,
                        right: index as f64 * 2.0 + 1.0,
                        top: 0.0,
                        bottom: 1.0,
                    },
                )
            })
            .collect::<Vec<_>>();
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 100)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error = a_star_path(
            &LayoutPoint { x: -10.0, y: 0.5 },
            &LayoutPoint { x: 100.0, y: 0.5 },
            &obstacles,
            &[],
            &[],
            &mut budget,
        )
        .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 5_712);
        assert_eq!(error.max, 100);
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn repeated_a_star_setup_consumes_one_cumulative_budget() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 65)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();
        let point = LayoutPoint { x: 0.0, y: 0.0 };

        assert!(
            a_star_path(&point, &point, &[], &[], &[], &mut budget)
                .unwrap()
                .is_some()
        );
        assert!(
            a_star_path(&point, &point, &[], &[], &[], &mut budget)
                .unwrap()
                .is_some()
        );
        assert_eq!(budget.used(), 50);

        let error = a_star_path(&point, &point, &[], &[], &[], &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 90);
        assert_eq!(error.max, 65);
        assert_eq!(budget.used(), 50);
    }

    #[test]
    fn track_budget_failure_does_not_commit_routed_points() {
        let mut layout = layout(
            vec![
                node("source", 0.0, 0.0, 40.0, 40.0),
                node("target", 0.0, 200.0, 40.0, 40.0),
            ],
            vec![
                edge("first", "source", "target"),
                edge("second", "source", "target"),
            ],
        );
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 30)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error = route(&mut layout, &mut budget).unwrap_err();

        assert!(
            error.to_string().contains("max_layout_work_units"),
            "{error}"
        );
        assert!(
            layout
                .original_edges
                .iter()
                .all(|edge| edge.points.is_empty()),
            "routing must commit only after track assignment succeeds"
        );
    }

    #[test]
    fn detours_an_anchor_that_starts_inside_a_foreign_obstacle() {
        let source = node("source", 0.0, 0.0, 40.0, 40.0);
        // The source bottom anchor is (0, 40). The padded blocker spans
        // x=[-28, 28], y=[32, 78], so that anchor is buried inside it.
        let blocker = node("blocker", 0.0, 55.0, 40.0, 30.0);
        let target = node("target", 0.0, 200.0, 40.0, 40.0);
        let mut layout = layout(
            vec![source, blocker.clone(), target],
            vec![edge("edge", "source", "target")],
        );

        route_unbounded(&mut layout);

        let points = &layout.original_edges[0].points;
        assert!(
            points.len() >= 4,
            "expected an orthogonal handle detour: {points:?}"
        );
        assert!(
            (points[0].x - points[1].x).abs() <= EPSILON,
            "the first renderer-facing handle must leave the source orthogonally: {points:?}"
        );
        let blocker_rect = Rect::from_center(blocker.x, blocker.y, blocker.width, blocker.height)
            .inflated(ROUTER_NODE_PADDING);
        assert!(
            points.windows(2).all(|segment| {
                !segment_blocked(
                    &segment[0],
                    &segment[1],
                    &[("blocker".to_string(), blocker_rect)],
                    &[],
                )
            }),
            "the routed polyline must clear the foreign obstacle: {points:?}"
        );
    }

    #[test]
    fn detours_an_anchor_that_ends_inside_a_foreign_obstacle() {
        let source = node("source", 0.0, 0.0, 40.0, 40.0);
        let blocker = node("blocker", 0.0, 145.0, 40.0, 30.0);
        // The target top anchor is (0, 160). The padded blocker spans
        // x=[-28, 28], y=[122, 168], so that anchor is buried inside it.
        let target = node("target", 0.0, 200.0, 40.0, 40.0);
        let mut layout = layout(
            vec![source, blocker.clone(), target],
            vec![edge("edge", "source", "target")],
        );

        route_unbounded(&mut layout);

        let points = &layout.original_edges[0].points;
        assert!(
            points.len() >= 4,
            "expected an orthogonal handle detour: {points:?}"
        );
        assert!(
            (points[points.len() - 2].x - points[points.len() - 1].x).abs() <= EPSILON,
            "the last renderer-facing handle must enter the target orthogonally: {points:?}"
        );
        let blocker_rect = Rect::from_center(blocker.x, blocker.y, blocker.width, blocker.height)
            .inflated(ROUTER_NODE_PADDING);
        assert!(
            points.windows(2).all(|segment| {
                !segment_blocked(
                    &segment[0],
                    &segment[1],
                    &[("blocker".to_string(), blocker_rect)],
                    &[],
                )
            }),
            "the routed polyline must clear the foreign obstacle: {points:?}"
        );
    }

    #[test]
    fn detours_a_horizontal_anchor_inside_a_foreign_obstacle() {
        let source = node("source", 0.0, 0.0, 40.0, 40.0);
        // The source right anchor is (40, 0). The padded blocker spans
        // x=[32, 78], y=[-28, 28], so that anchor is buried inside it.
        let blocker = node("blocker", 55.0, 0.0, 30.0, 40.0);
        let target = node("target", 200.0, 0.0, 40.0, 40.0);
        let mut layout = layout(
            vec![source, blocker.clone(), target],
            vec![edge("edge", "source", "target")],
        );

        route_unbounded(&mut layout);

        let points = &layout.original_edges[0].points;
        assert!(
            points.len() >= 4,
            "expected an orthogonal handle detour: {points:?}"
        );
        assert!(
            (points[0].y - points[1].y).abs() <= EPSILON,
            "the first renderer-facing handle must leave the source orthogonally: {points:?}"
        );
        let blocker_rect = Rect::from_center(blocker.x, blocker.y, blocker.width, blocker.height)
            .inflated(ROUTER_NODE_PADDING);
        assert!(
            points.windows(2).all(|segment| {
                !segment_blocked(
                    &segment[0],
                    &segment[1],
                    &[("blocker".to_string(), blocker_rect)],
                    &[],
                )
            }),
            "the routed polyline must clear the foreign obstacle: {points:?}"
        );
    }
}
