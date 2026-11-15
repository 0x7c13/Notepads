use super::super::working::WorkingLayout;
use super::geometry::{
    EPSILON, RectBounds, dedupe_consecutive_points, orthogonalize_polyline, rect_of_node_bounds,
    same_point, same_x, same_y, simplify_polyline,
};
use crate::model::LayoutPoint;
use std::collections::HashMap;

const INSIDE_EPSILON: f64 = 0.5;
const CORNER_CLEARANCE: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BorderSide {
    Top,
    Bottom,
    Left,
    Right,
}

fn segment_enter_point(
    outside: &LayoutPoint,
    inside: &LayoutPoint,
    rect: RectBounds,
) -> LayoutPoint {
    if same_y(outside, inside, EPSILON) {
        return LayoutPoint {
            x: if outside.x < rect.left {
                rect.left
            } else {
                rect.right
            },
            y: outside.y,
        };
    }
    if same_x(outside, inside, EPSILON) {
        return LayoutPoint {
            x: outside.x,
            y: if outside.y < rect.top {
                rect.top
            } else {
                rect.bottom
            },
        };
    }
    LayoutPoint {
        x: outside.x.clamp(rect.left, rect.right),
        y: outside.y.clamp(rect.top, rect.bottom),
    }
}

fn clip_endpoint(points: &[LayoutPoint], rect: RectBounds, at_start: bool) -> Vec<LayoutPoint> {
    let step: isize = if at_start { 1 } else { -1 };
    let mut outside_index = if at_start {
        0
    } else {
        points.len() as isize - 1
    };
    while outside_index >= 0
        && (outside_index as usize) < points.len()
        && rect.contains_point(&points[outside_index as usize], INSIDE_EPSILON)
    {
        outside_index += step;
    }
    if outside_index < 0 || outside_index as usize >= points.len() {
        return points.to_vec();
    }

    let inside_index = outside_index - step;
    if inside_index < 0 || inside_index as usize >= points.len() {
        return points.to_vec();
    }

    let entry = segment_enter_point(
        &points[outside_index as usize],
        &points[inside_index as usize],
        rect,
    );
    if at_start {
        let mut result = Vec::with_capacity(points.len() - outside_index as usize + 1);
        result.push(entry);
        result.extend_from_slice(&points[outside_index as usize..]);
        result
    } else {
        let mut result = points[..=outside_index as usize].to_vec();
        result.push(entry);
        result
    }
}

fn node_rects(layout: &WorkingLayout) -> HashMap<String, RectBounds> {
    layout
        .nodes
        .values()
        .filter_map(|node| Some((node.id.clone(), rect_of_node_bounds(node)?)))
        .collect()
}

pub(super) fn clip_edge_endpoints_to_node_boundaries(layout: &mut WorkingLayout) {
    let rects = node_rects(layout);
    for edge in &mut layout.original_edges {
        if edge.points.len() < 2 {
            continue;
        }
        let source_rect = rects.get(&edge.from).copied();
        let destination_rect = rects.get(&edge.to).copied();
        let mut next = edge.points.clone();
        if let Some(rect) = source_rect {
            next = clip_endpoint(&next, rect, true);
        }
        if let Some(rect) = destination_rect {
            next = clip_endpoint(&next, rect, false);
        }
        next = simplify_polyline(&orthogonalize_polyline(&next));
        next = clear_straight_endpoint_corner_connections(&next, source_rect, destination_rect);
        edge.points = simplify_polyline(&orthogonalize_polyline(&next));
    }
}

fn snap_endpoint_to_boundary(
    inner: &LayoutPoint,
    endpoint: &LayoutPoint,
    rect: RectBounds,
    use_approach_side: bool,
) -> LayoutPoint {
    if same_y(inner, endpoint, EPSILON) {
        if endpoint.y < rect.top - EPSILON || endpoint.y > rect.bottom + EPSILON {
            return endpoint.clone();
        }
        if use_approach_side {
            if inner.x < rect.left - EPSILON {
                return LayoutPoint {
                    x: rect.left,
                    y: inner.y,
                };
            }
            if inner.x > rect.right + EPSILON {
                return LayoutPoint {
                    x: rect.right,
                    y: inner.y,
                };
            }
        }
        let to_left = (endpoint.x - rect.left).abs() <= (endpoint.x - rect.right).abs();
        return LayoutPoint {
            x: if to_left { rect.left } else { rect.right },
            y: inner.y,
        };
    }
    if same_x(inner, endpoint, EPSILON) {
        if endpoint.x < rect.left - EPSILON || endpoint.x > rect.right + EPSILON {
            return endpoint.clone();
        }
        if use_approach_side {
            if inner.y < rect.top - EPSILON {
                return LayoutPoint {
                    x: inner.x,
                    y: rect.top,
                };
            }
            if inner.y > rect.bottom + EPSILON {
                return LayoutPoint {
                    x: inner.x,
                    y: rect.bottom,
                };
            }
        }
        let to_top = (endpoint.y - rect.top).abs() <= (endpoint.y - rect.bottom).abs();
        return LayoutPoint {
            x: inner.x,
            y: if to_top { rect.top } else { rect.bottom },
        };
    }
    endpoint.clone()
}

fn first_distinct_adjacent(
    points: &[LayoutPoint],
    endpoint_index: usize,
    step: isize,
) -> Option<LayoutPoint> {
    let endpoint = points.get(endpoint_index)?;
    let mut index = endpoint_index as isize + step;
    while index >= 0 && (index as usize) < points.len() {
        let candidate = &points[index as usize];
        if !same_point(candidate, endpoint, EPSILON) {
            return Some(candidate.clone());
        }
        index += step;
    }
    let fallback = endpoint_index as isize + step;
    (fallback >= 0)
        .then(|| points.get(fallback as usize))
        .flatten()
        .cloned()
}

fn corner_clearance_range(minimum: f64, maximum: f64) -> (f64, f64) {
    let low = minimum + CORNER_CLEARANCE;
    let high = maximum - CORNER_CLEARANCE;
    if low <= high {
        (low, high)
    } else {
        let center = (minimum + maximum) / 2.0;
        (center, center)
    }
}

fn clamp_to_corner_clearance(value: f64, minimum: f64, maximum: f64) -> f64 {
    let (low, high) = corner_clearance_range(minimum, maximum);
    value.clamp(low, high)
}

fn intersect_ranges(ranges: &[(f64, f64)]) -> Option<(f64, f64)> {
    let low = ranges.iter().map(|range| range.0).reduce(f64::max)?;
    let high = ranges.iter().map(|range| range.1).reduce(f64::min)?;
    (low <= high).then_some((low, high))
}

fn clearance_range_for_side(rect: RectBounds, side: BorderSide) -> (f64, f64) {
    if is_horizontal_side(side) {
        corner_clearance_range(rect.top, rect.bottom)
    } else {
        corner_clearance_range(rect.left, rect.right)
    }
}

fn terminal_side_for_segment(
    endpoint: &LayoutPoint,
    adjacent: &LayoutPoint,
    rect: RectBounds,
) -> Option<BorderSide> {
    let y_within = endpoint.y >= rect.top - EPSILON && endpoint.y <= rect.bottom + EPSILON;
    let x_within = endpoint.x >= rect.left - EPSILON && endpoint.x <= rect.right + EPSILON;
    if same_y(endpoint, adjacent, EPSILON) && y_within {
        if (endpoint.x - rect.left).abs() < EPSILON {
            return Some(BorderSide::Left);
        }
        if (endpoint.x - rect.right).abs() < EPSILON {
            return Some(BorderSide::Right);
        }
    }
    if same_x(endpoint, adjacent, EPSILON) && x_within {
        if (endpoint.y - rect.top).abs() < EPSILON {
            return Some(BorderSide::Top);
        }
        if (endpoint.y - rect.bottom).abs() < EPSILON {
            return Some(BorderSide::Bottom);
        }
    }
    None
}

fn is_horizontal_side(side: BorderSide) -> bool {
    matches!(side, BorderSide::Left | BorderSide::Right)
}

fn straight_clearance_range(
    start: &LayoutPoint,
    end: &LayoutPoint,
    source_rect: Option<RectBounds>,
    destination_rect: Option<RectBounds>,
    horizontal: bool,
) -> Option<(f64, f64)> {
    let mut ranges = Vec::with_capacity(2);
    let source_side = source_rect.and_then(|rect| terminal_side_for_segment(start, end, rect));
    let destination_side =
        destination_rect.and_then(|rect| terminal_side_for_segment(end, start, rect));
    if let (Some(rect), Some(side)) = (source_rect, source_side)
        && is_horizontal_side(side) == horizontal
    {
        ranges.push(clearance_range_for_side(rect, side));
    }
    if let (Some(rect), Some(side)) = (destination_rect, destination_side)
        && is_horizontal_side(side) == horizontal
    {
        ranges.push(clearance_range_for_side(rect, side));
    }
    intersect_ranges(&ranges)
}

fn clear_straight_endpoint_corner_axis(
    start: &LayoutPoint,
    end: &LayoutPoint,
    source_rect: Option<RectBounds>,
    destination_rect: Option<RectBounds>,
    horizontal: bool,
) -> Option<Vec<LayoutPoint>> {
    let (low, high) =
        straight_clearance_range(start, end, source_rect, destination_rect, horizontal)?;
    let current = if horizontal { start.y } else { start.x };
    let next = current.clamp(low, high);
    if (next - current).abs() < EPSILON {
        return None;
    }
    Some(if horizontal {
        vec![
            LayoutPoint {
                x: start.x,
                y: next,
            },
            LayoutPoint { x: end.x, y: next },
        ]
    } else {
        vec![
            LayoutPoint {
                x: next,
                y: start.y,
            },
            LayoutPoint { x: next, y: end.y },
        ]
    })
}

fn clear_straight_endpoint_corner_connections(
    points: &[LayoutPoint],
    source_rect: Option<RectBounds>,
    destination_rect: Option<RectBounds>,
) -> Vec<LayoutPoint> {
    if points.len() != 2 {
        return points.to_vec();
    }
    let start = &points[0];
    let end = &points[1];
    if same_y(start, end, EPSILON) {
        return clear_straight_endpoint_corner_axis(
            start,
            end,
            source_rect,
            destination_rect,
            true,
        )
        .unwrap_or_else(|| points.to_vec());
    }
    if same_x(start, end, EPSILON) {
        return clear_straight_endpoint_corner_axis(
            start,
            end,
            source_rect,
            destination_rect,
            false,
        )
        .unwrap_or_else(|| points.to_vec());
    }
    points.to_vec()
}

fn corner_cleared_endpoint(
    endpoint: &LayoutPoint,
    rect: RectBounds,
    side: BorderSide,
) -> LayoutPoint {
    if is_horizontal_side(side) {
        LayoutPoint {
            x: endpoint.x,
            y: clamp_to_corner_clearance(endpoint.y, rect.top, rect.bottom),
        }
    } else {
        LayoutPoint {
            x: clamp_to_corner_clearance(endpoint.x, rect.left, rect.right),
            y: endpoint.y,
        }
    }
}

fn move_collinear_endpoint_run(
    points: &[LayoutPoint],
    endpoint_index: usize,
    step: isize,
    endpoint: &LayoutPoint,
    adjusted: &LayoutPoint,
    horizontal_terminal: bool,
) -> Vec<LayoutPoint> {
    let mut next = points.to_vec();
    let mut index = endpoint_index as isize;
    while index >= 0 && (index as usize) < points.len() {
        let point = &points[index as usize];
        if horizontal_terminal && !same_y(point, endpoint, EPSILON) {
            break;
        }
        if !horizontal_terminal && !same_x(point, endpoint, EPSILON) {
            break;
        }
        if horizontal_terminal {
            next[index as usize].y = adjusted.y;
        } else {
            next[index as usize].x = adjusted.x;
        }
        index += step;
    }
    next
}

fn clear_endpoint_corner_connection(
    points: &[LayoutPoint],
    rect: RectBounds,
    at_start: bool,
) -> Vec<LayoutPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let endpoint_index = if at_start { 0 } else { points.len() - 1 };
    let step = if at_start { 1 } else { -1 };
    let endpoint = &points[endpoint_index];
    let Some(adjacent) = first_distinct_adjacent(points, endpoint_index, step) else {
        return points.to_vec();
    };
    let Some(side) = terminal_side_for_segment(endpoint, &adjacent, rect) else {
        return points.to_vec();
    };
    let horizontal_terminal = is_horizontal_side(side);
    let adjusted = corner_cleared_endpoint(endpoint, rect, side);
    if same_point(endpoint, &adjusted, EPSILON) {
        return points.to_vec();
    }
    move_collinear_endpoint_run(
        points,
        endpoint_index,
        step,
        endpoint,
        &adjusted,
        horizontal_terminal,
    )
}

fn border_side_for_segment(
    start: &LayoutPoint,
    end: &LayoutPoint,
    rect: RectBounds,
) -> Option<BorderSide> {
    let x_within =
        start.x.min(end.x) >= rect.left - EPSILON && start.x.max(end.x) <= rect.right + EPSILON;
    let y_within =
        start.y.min(end.y) >= rect.top - EPSILON && start.y.max(end.y) <= rect.bottom + EPSILON;
    if (start.y - rect.top).abs() < EPSILON && (end.y - rect.top).abs() < EPSILON && x_within {
        return Some(BorderSide::Top);
    }
    if (start.y - rect.bottom).abs() < EPSILON && (end.y - rect.bottom).abs() < EPSILON && x_within
    {
        return Some(BorderSide::Bottom);
    }
    if (start.x - rect.left).abs() < EPSILON && (end.x - rect.left).abs() < EPSILON && y_within {
        return Some(BorderSide::Left);
    }
    if (start.x - rect.right).abs() < EPSILON && (end.x - rect.right).abs() < EPSILON && y_within {
        return Some(BorderSide::Right);
    }
    None
}

fn leaves_outward(
    side: BorderSide,
    from: &LayoutPoint,
    to: &LayoutPoint,
    rect: RectBounds,
) -> bool {
    match side {
        BorderSide::Top => same_x(from, to, EPSILON) && to.y < rect.top - EPSILON,
        BorderSide::Bottom => same_x(from, to, EPSILON) && to.y > rect.bottom + EPSILON,
        BorderSide::Left => same_y(from, to, EPSILON) && to.x < rect.left - EPSILON,
        BorderSide::Right => same_y(from, to, EPSILON) && to.x > rect.right + EPSILON,
    }
}

fn collapse_own_border_stub(
    points: &[LayoutPoint],
    rect: RectBounds,
    at_start: bool,
) -> Vec<LayoutPoint> {
    if points.len() < 3 {
        return points.to_vec();
    }
    if at_start {
        if let Some(side) = border_side_for_segment(&points[0], &points[1], rect)
            && leaves_outward(side, &points[1], &points[2], rect)
        {
            return points[1..].to_vec();
        }
        return points.to_vec();
    }

    let last = points.len() - 1;
    if let Some(side) = border_side_for_segment(&points[last - 1], &points[last], rect)
        && leaves_outward(side, &points[last - 1], &points[last - 2], rect)
    {
        return points[..last].to_vec();
    }
    points.to_vec()
}

fn snap_and_collapse_endpoints(
    points: &[LayoutPoint],
    source_rect: Option<RectBounds>,
    destination_rect: Option<RectBounds>,
) -> Vec<LayoutPoint> {
    let mut next = points.to_vec();
    if let Some(rect) = source_rect {
        if let Some(adjacent) = first_distinct_adjacent(&next, 0, 1) {
            next[0] = snap_endpoint_to_boundary(&adjacent, &next[0], rect, false);
        }
        next = collapse_own_border_stub(&next, rect, true);
    }
    if let Some(rect) = destination_rect {
        let last = next.len() - 1;
        if let Some(adjacent) = first_distinct_adjacent(&next, last, -1) {
            next[last] = snap_endpoint_to_boundary(&adjacent, &next[last], rect, true);
        }
        next = collapse_own_border_stub(&next, rect, false);
    }

    let straight_cleared =
        clear_straight_endpoint_corner_connections(&next, source_rect, destination_rect);
    if next.len() == 2 {
        return straight_cleared;
    }
    next = straight_cleared;
    if let Some(rect) = source_rect {
        next = clear_endpoint_corner_connection(&next, rect, true);
    }
    if let Some(rect) = destination_rect {
        next = clear_endpoint_corner_connection(&next, rect, false);
    }
    next
}

pub(super) fn prepare_edge_endpoints_for_renderer(layout: &mut WorkingLayout) {
    let rects = node_rects(layout);
    for edge in &mut layout.original_edges {
        if edge.points.len() < 2 {
            continue;
        }
        let source_rect = rects.get(&edge.from).copied();
        let destination_rect = rects.get(&edge.to).copied();
        let input = dedupe_consecutive_points(&edge.points, EPSILON);
        let new_points = snap_and_collapse_endpoints(&input, source_rect, destination_rect);
        if new_points.len() < 3 {
            edge.points = new_points;
            continue;
        }
        let mut duplicated = Vec::with_capacity(new_points.len() + 2);
        duplicated.push(new_points[0].clone());
        duplicated.push(new_points[0].clone());
        duplicated.extend_from_slice(&new_points[1..new_points.len() - 1]);
        duplicated.push(new_points[new_points.len() - 1].clone());
        duplicated.push(new_points[new_points.len() - 1].clone());
        edge.points = duplicated;
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::working::{WorkingEdge, WorkingNode, WorkingNodeKind};
    use super::*;
    use crate::model::SwimlaneDirection;
    use indexmap::IndexMap;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
    }

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

    fn layout(nodes: Vec<WorkingNode>, points: Vec<LayoutPoint>) -> WorkingLayout {
        let nodes = nodes
            .into_iter()
            .map(|node| (node.id.clone(), node))
            .collect::<IndexMap<_, _>>();
        WorkingLayout {
            direction: SwimlaneDirection::Tb,
            nodes,
            graph_edges: Vec::new(),
            original_edges: vec![WorkingEdge {
                id: "A_B".to_string(),
                from: "A".to_string(),
                to: "B".to_string(),
                reference_id: "A_B".to_string(),
                label_node_id: None,
                reversed_for_layout: false,
                points,
            }],
            top_lane_order: Vec::new(),
        }
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (point, &(x, y)) in actual.iter().zip(expected) {
            assert!((point.x - x).abs() < EPSILON, "x: {} != {x}", point.x);
            assert!((point.y - y).abs() < EPSILON, "y: {} != {y}", point.y);
        }
    }

    #[test]
    fn clips_buried_endpoints_to_source_and_destination_boundaries() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 10.0, 10.0),
                node("B", 20.0, 0.0, 10.0, 10.0),
            ],
            vec![point(0.0, 0.0), point(20.0, 0.0)],
        );
        clip_edge_endpoints_to_node_boundaries(&mut layout);
        assert_points(&layout.original_edges[0].points, &[(5.0, 0.0), (15.0, 0.0)]);
    }

    #[test]
    fn moves_straight_side_to_side_endpoints_away_from_node_corners() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 20.0, 20.0),
                node("B", 40.0, 0.0, 20.0, 40.0),
            ],
            vec![point(10.0, 8.0), point(30.0, 8.0)],
        );
        clip_edge_endpoints_to_node_boundaries(&mut layout);
        assert_points(
            &layout.original_edges[0].points,
            &[(10.0, 6.0), (30.0, 6.0)],
        );
    }

    #[test]
    fn duplicates_snapped_endpoints_for_renderer_clipping() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 10.0, 10.0),
                node("B", 20.0, 0.0, 10.0, 10.0),
            ],
            vec![point(-5.0, 0.0), point(10.0, 0.0), point(15.0, 0.0)],
        );
        prepare_edge_endpoints_for_renderer(&mut layout);
        assert_points(
            &layout.original_edges[0].points,
            &[
                (-5.0, 0.0),
                (-5.0, 0.0),
                (10.0, 0.0),
                (15.0, 0.0),
                (15.0, 0.0),
            ],
        );
    }

    #[test]
    fn renderer_endpoint_materialization_is_idempotent() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 10.0, 10.0),
                node("B", 20.0, 0.0, 10.0, 10.0),
            ],
            vec![point(-5.0, 0.0), point(10.0, 0.0), point(15.0, 0.0)],
        );
        prepare_edge_endpoints_for_renderer(&mut layout);
        let once = layout.original_edges[0].points.clone();
        prepare_edge_endpoints_for_renderer(&mut layout);
        assert_points(
            &layout.original_edges[0].points,
            &once
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn keeps_straight_renderer_edges_two_point_while_clearing_corner_ports() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 20.0, 20.0),
                node("B", 40.0, 0.0, 20.0, 40.0),
            ],
            vec![point(10.0, 8.0), point(30.0, 8.0)],
        );
        prepare_edge_endpoints_for_renderer(&mut layout);
        assert_points(
            &layout.original_edges[0].points,
            &[(10.0, 6.0), (30.0, 6.0)],
        );
    }

    #[test]
    fn snaps_renderer_endpoint_to_boundary_entered_by_approach_segment() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 10.0, 10.0),
                node("B", 40.0, 40.0, 20.0, 20.0),
            ],
            vec![point(5.0, 0.0), point(40.0, 0.0), point(40.0, 50.0)],
        );
        prepare_edge_endpoints_for_renderer(&mut layout);
        assert_points(
            &layout.original_edges[0].points,
            &[
                (5.0, 0.0),
                (5.0, 0.0),
                (40.0, 0.0),
                (40.0, 30.0),
                (40.0, 30.0),
            ],
        );
    }
}
