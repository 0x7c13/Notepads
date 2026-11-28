use super::super::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
use crate::model::LayoutPoint;
use crate::swimlane::geometry::Rect;
use indexmap::IndexMap;

pub(in crate::swimlane) const EPSILON: f64 = 1e-3;

pub(super) type RectBounds = Rect;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RectEntry {
    pub id: String,
    pub rect: RectBounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RectSide {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NodeBoundsInfo {
    pub id: String,
    pub rect: RectBounds,
    pub cx: f64,
    pub cy: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NodePairGeometry {
    pub src_id: String,
    pub dst_id: String,
    pub src_info: NodeBoundsInfo,
    pub dst_info: NodeBoundsInfo,
    pub collinear_x: bool,
    pub collinear_y: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct LayoutNodeRect {
    pub node_id: String,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ThreeSegmentRouteKind {
    Hvh,
    Vhv,
}

#[derive(Debug, Clone)]
pub(super) struct ThreeSegmentRoute {
    pub kind: ThreeSegmentRouteKind,
    pub p3: LayoutPoint,
}

#[derive(Debug, Clone)]
pub(super) struct OrthogonalSegment {
    pub index: usize,
    pub a: LayoutPoint,
    pub b: LayoutPoint,
    pub horizontal: bool,
    pub vertical: bool,
}

pub(super) fn same_point(left: &LayoutPoint, right: &LayoutPoint, epsilon: f64) -> bool {
    (left.x - right.x).abs() < epsilon && (left.y - right.y).abs() < epsilon
}

pub(super) fn same_x(left: &LayoutPoint, right: &LayoutPoint, epsilon: f64) -> bool {
    (left.x - right.x).abs() < epsilon
}

pub(super) fn same_y(left: &LayoutPoint, right: &LayoutPoint, epsilon: f64) -> bool {
    (left.y - right.y).abs() < epsilon
}

pub(super) fn is_horizontal_segment(start: &LayoutPoint, end: &LayoutPoint, epsilon: f64) -> bool {
    same_y(start, end, epsilon) && (start.x - end.x).abs() > epsilon
}

pub(super) fn is_vertical_segment(start: &LayoutPoint, end: &LayoutPoint, epsilon: f64) -> bool {
    same_x(start, end, epsilon) && (start.y - end.y).abs() > epsilon
}

pub(super) fn overlap_length(
    first_start: f64,
    first_end: f64,
    second_start: f64,
    second_end: f64,
) -> f64 {
    (first_start.max(first_end).min(second_start.max(second_end))
        - first_start.min(first_end).max(second_start.min(second_end)))
    .max(0.0)
}

pub(super) fn same_axis_segment_overlap_length(
    first: &OrthogonalSegment,
    second: &OrthogonalSegment,
    epsilon: f64,
) -> f64 {
    if first.horizontal && second.horizontal && same_y(&first.a, &second.a, epsilon) {
        return overlap_length(first.a.x, first.b.x, second.a.x, second.b.x);
    }
    if first.vertical && second.vertical && same_x(&first.a, &second.a, epsilon) {
        return overlap_length(first.a.y, first.b.y, second.a.y, second.b.y);
    }
    0.0
}

pub(super) fn orthogonal_segments_for_points(
    points: &[LayoutPoint],
    epsilon: f64,
) -> Vec<OrthogonalSegment> {
    points
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let horizontal = is_horizontal_segment(&pair[0], &pair[1], epsilon);
            let vertical = is_vertical_segment(&pair[0], &pair[1], epsilon);
            (horizontal || vertical).then(|| OrthogonalSegment {
                index,
                a: pair[0].clone(),
                b: pair[1].clone(),
                horizontal,
                vertical,
            })
        })
        .collect()
}

pub(super) fn count_orthogonal_bends(points: &[LayoutPoint], epsilon: f64) -> usize {
    orthogonal_segments_for_points(points, epsilon)
        .windows(2)
        .filter(|segments| segments[0].horizontal != segments[1].horizontal)
        .count()
}

pub(super) fn dedupe_consecutive_points(points: &[LayoutPoint], epsilon: f64) -> Vec<LayoutPoint> {
    let mut result = Vec::with_capacity(points.len());
    for point in points {
        if result
            .last()
            .is_none_or(|last| !same_point(last, point, epsilon))
        {
            result.push(point.clone());
        }
    }
    result
}

pub(super) fn classify_three_segment_route(
    points: &[LayoutPoint],
    epsilon: f64,
) -> Option<ThreeSegmentRoute> {
    let [p0, p1, p2, p3] = points else {
        return None;
    };
    let kind = if is_horizontal_segment(p0, p1, epsilon)
        && is_vertical_segment(p1, p2, epsilon)
        && is_horizontal_segment(p2, p3, epsilon)
    {
        ThreeSegmentRouteKind::Hvh
    } else if is_vertical_segment(p0, p1, epsilon)
        && is_horizontal_segment(p1, p2, epsilon)
        && is_vertical_segment(p2, p3, epsilon)
    {
        ThreeSegmentRouteKind::Vhv
    } else {
        return None;
    };
    Some(ThreeSegmentRoute {
        kind,
        p3: p3.clone(),
    })
}

pub(super) fn segment_bounds_overlap_rect(
    start: &LayoutPoint,
    end: &LayoutPoint,
    rect: RectBounds,
    buffer: f64,
) -> bool {
    let segment_min_x = start.x.min(end.x);
    let segment_max_x = start.x.max(end.x);
    let segment_min_y = start.y.min(end.y);
    let segment_max_y = start.y.max(end.y);
    segment_max_x > rect.left - buffer
        && segment_min_x < rect.right + buffer
        && segment_max_y > rect.top - buffer
        && segment_min_y < rect.bottom + buffer
}

pub(super) fn rect_contains_rect(outer: RectBounds, inner: RectBounds) -> bool {
    outer.left <= inner.left
        && outer.right >= inner.right
        && outer.top <= inner.top
        && outer.bottom >= inner.bottom
}

pub(super) fn rects_overlap(first: RectBounds, second: RectBounds) -> bool {
    first.left < second.right
        && first.right > second.left
        && first.top < second.bottom
        && first.bottom > second.top
}

pub(super) fn rect_of_node_bounds(node: &WorkingNode) -> Option<RectBounds> {
    (node.width > 0.0 && node.height > 0.0)
        .then(|| RectBounds::from_center(node.x, node.y, node.width, node.height))
}

fn node_bounds_info_for(node: &WorkingNode) -> Option<NodeBoundsInfo> {
    if node.kind == WorkingNodeKind::Group {
        return None;
    }
    Some(NodeBoundsInfo {
        id: node.id.clone(),
        rect: rect_of_node_bounds(node)?,
        cx: node.x,
        cy: node.y,
    })
}

pub(super) fn port_for_rect_side(node: &NodeBoundsInfo, side: RectSide) -> LayoutPoint {
    match side {
        RectSide::Top => LayoutPoint {
            x: node.cx,
            y: node.rect.top,
        },
        RectSide::Bottom => LayoutPoint {
            x: node.cx,
            y: node.rect.bottom,
        },
        RectSide::Left => LayoutPoint {
            x: node.rect.left,
            y: node.cy,
        },
        RectSide::Right => LayoutPoint {
            x: node.rect.right,
            y: node.cy,
        },
    }
}

pub(super) fn build_orthogonal_port_path(
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    anchor: f64,
    epsilon: f64,
) -> Option<Vec<LayoutPoint>> {
    let source_horizontal = matches!(source_side, RectSide::Left | RectSide::Right);
    let destination_horizontal = matches!(destination_side, RectSide::Left | RectSide::Right);

    if source_horizontal && destination_horizontal {
        let opposing_direction = (source_side == RectSide::Right
            && destination_side == RectSide::Left
            && source.x < destination.x)
            || (source_side == RectSide::Left
                && destination_side == RectSide::Right
                && source.x > destination.x);
        if opposing_direction {
            if same_y(source, destination, epsilon) {
                return Some(vec![source.clone(), destination.clone()]);
            }
            let middle_x = (source.x + destination.x) / 2.0;
            return Some(vec![
                source.clone(),
                LayoutPoint {
                    x: middle_x,
                    y: source.y,
                },
                LayoutPoint {
                    x: middle_x,
                    y: destination.y,
                },
                destination.clone(),
            ]);
        }
        if source_side == destination_side {
            if same_y(source, destination, epsilon) {
                return None;
            }
            let intermediate_x = if source_side == RectSide::Left {
                source.x.min(destination.x) - anchor
            } else {
                source.x.max(destination.x) + anchor
            };
            return Some(vec![
                source.clone(),
                LayoutPoint {
                    x: intermediate_x,
                    y: source.y,
                },
                LayoutPoint {
                    x: intermediate_x,
                    y: destination.y,
                },
                destination.clone(),
            ]);
        }
        return None;
    }

    if !source_horizontal && !destination_horizontal {
        if source_side == destination_side {
            if same_x(source, destination, epsilon) {
                return None;
            }
            let intermediate_y = if source_side == RectSide::Top {
                source.y.min(destination.y) - anchor
            } else {
                source.y.max(destination.y) + anchor
            };
            return Some(vec![
                source.clone(),
                LayoutPoint {
                    x: source.x,
                    y: intermediate_y,
                },
                LayoutPoint {
                    x: destination.x,
                    y: intermediate_y,
                },
                destination.clone(),
            ]);
        }
        let same_direction = (source_side == RectSide::Bottom
            && destination_side == RectSide::Top
            && source.y < destination.y)
            || (source_side == RectSide::Top
                && destination_side == RectSide::Bottom
                && source.y > destination.y);
        if !same_direction {
            return None;
        }
        if same_x(source, destination, epsilon) {
            return Some(vec![source.clone(), destination.clone()]);
        }
        let middle_y = (source.y + destination.y) / 2.0;
        return Some(vec![
            source.clone(),
            LayoutPoint {
                x: source.x,
                y: middle_y,
            },
            LayoutPoint {
                x: destination.x,
                y: middle_y,
            },
            destination.clone(),
        ]);
    }

    if source_horizontal {
        let source_same_direction = (source_side == RectSide::Right && destination.x > source.x)
            || (source_side == RectSide::Left && destination.x < source.x);
        let destination_same_direction = (destination_side == RectSide::Top
            && source.y < destination.y)
            || (destination_side == RectSide::Bottom && source.y > destination.y);
        return (source_same_direction && destination_same_direction).then(|| {
            vec![
                source.clone(),
                LayoutPoint {
                    x: destination.x,
                    y: source.y,
                },
                destination.clone(),
            ]
        });
    }

    let source_same_direction = (source_side == RectSide::Bottom && destination.y > source.y)
        || (source_side == RectSide::Top && destination.y < source.y);
    let destination_same_direction = (destination_side == RectSide::Left
        && source.x < destination.x)
        || (destination_side == RectSide::Right && source.x > destination.x);
    (source_same_direction && destination_same_direction).then(|| {
        vec![
            source.clone(),
            LayoutPoint {
                x: source.x,
                y: destination.y,
            },
            destination.clone(),
        ]
    })
}

pub(super) fn build_same_side_track_path(
    source: &LayoutPoint,
    side: RectSide,
    destination: &LayoutPoint,
    track: f64,
) -> Vec<LayoutPoint> {
    if matches!(side, RectSide::Left | RectSide::Right) {
        vec![
            source.clone(),
            LayoutPoint {
                x: track,
                y: source.y,
            },
            LayoutPoint {
                x: track,
                y: destination.y,
            },
            destination.clone(),
        ]
    } else {
        vec![
            source.clone(),
            LayoutPoint {
                x: source.x,
                y: track,
            },
            LayoutPoint {
                x: destination.x,
                y: track,
            },
            destination.clone(),
        ]
    }
}

pub(super) fn collect_real_node_bounds(
    layout: &WorkingLayout,
) -> (IndexMap<String, NodeBoundsInfo>, Vec<RectEntry>) {
    let mut node_info_by_id = IndexMap::new();
    let mut real_node_rects = Vec::new();
    for node in layout.nodes.values() {
        if node.kind == WorkingNodeKind::EdgeLabel {
            continue;
        }
        let Some(info) = node_bounds_info_for(node) else {
            continue;
        };
        real_node_rects.push(RectEntry {
            id: info.id.clone(),
            rect: info.rect,
        });
        node_info_by_id.insert(info.id.clone(), info);
    }
    (node_info_by_id, real_node_rects)
}

pub(super) fn collect_node_rect_entries(
    layout: &WorkingLayout,
) -> (Vec<RectEntry>, Vec<RectEntry>) {
    let mut real_node_rects = Vec::new();
    let mut label_node_rects = Vec::new();
    for node in layout.nodes.values() {
        let Some(info) = node_bounds_info_for(node) else {
            continue;
        };
        let entry = RectEntry {
            id: info.id,
            rect: info.rect,
        };
        if node.kind == WorkingNodeKind::EdgeLabel {
            label_node_rects.push(entry);
        } else {
            real_node_rects.push(entry);
        }
    }
    (real_node_rects, label_node_rects)
}

#[cfg(test)]
pub(super) fn collect_layout_node_rects(
    layout: &WorkingLayout,
    include_edge_labels: bool,
) -> Vec<LayoutNodeRect> {
    layout
        .nodes
        .values()
        .filter(|node| {
            node.kind != WorkingNodeKind::Group
                && (include_edge_labels || node.kind != WorkingNodeKind::EdgeLabel)
        })
        .map(|node| {
            let rect = RectBounds::from_center(node.x, node.y, node.width, node.height);
            LayoutNodeRect {
                node_id: node.id.clone(),
                left: rect.left,
                right: rect.right,
                top: rect.top,
                bottom: rect.bottom,
            }
        })
        .collect()
}

pub(super) fn get_node_pair_geometry(
    edge: &WorkingEdge,
    node_info_by_id: &IndexMap<String, NodeBoundsInfo>,
    epsilon: f64,
) -> Option<NodePairGeometry> {
    if edge.from.is_empty() || edge.to.is_empty() {
        return None;
    }
    let source = node_info_by_id.get(&edge.from)?;
    let destination = node_info_by_id.get(&edge.to)?;
    Some(NodePairGeometry {
        src_id: edge.from.clone(),
        dst_id: edge.to.clone(),
        src_info: source.clone(),
        dst_info: destination.clone(),
        collinear_x: (source.cx - destination.cx).abs() < epsilon,
        collinear_y: (source.cy - destination.cy).abs() < epsilon,
    })
}

pub(super) fn segment_hits_any_rect(
    start: &LayoutPoint,
    end: &LayoutPoint,
    rects: &[RectEntry],
    excluded_ids: &[&str],
    shrink: f64,
) -> bool {
    rects.iter().any(|entry| {
        !excluded_ids.contains(&entry.id.as_str())
            && segment_bounds_overlap_rect(start, end, entry.rect, -shrink)
    })
}

pub(in crate::swimlane) fn orthogonal_segments_strictly_cross(
    first_start: &LayoutPoint,
    first_end: &LayoutPoint,
    second_start: &LayoutPoint,
    second_end: &LayoutPoint,
    epsilon: f64,
) -> bool {
    let first_horizontal = same_y(first_start, first_end, epsilon);
    let first_vertical = same_x(first_start, first_end, epsilon);
    let second_horizontal = same_y(second_start, second_end, epsilon);
    let second_vertical = same_x(second_start, second_end, epsilon);
    if !((first_horizontal && second_vertical) || (first_vertical && second_horizontal)) {
        return false;
    }

    let (horizontal_start, horizontal_end, vertical_start, vertical_end) = if first_horizontal {
        (first_start, first_end, second_start, second_end)
    } else {
        (second_start, second_end, first_start, first_end)
    };
    let horizontal_y = horizontal_start.y;
    let horizontal_min_x = horizontal_start.x.min(horizontal_end.x);
    let horizontal_max_x = horizontal_start.x.max(horizontal_end.x);
    let vertical_x = vertical_start.x;
    let vertical_min_y = vertical_start.y.min(vertical_end.y);
    let vertical_max_y = vertical_start.y.max(vertical_end.y);
    vertical_x > horizontal_min_x + epsilon
        && vertical_x < horizontal_max_x - epsilon
        && horizontal_y > vertical_min_y + epsilon
        && horizontal_y < vertical_max_y - epsilon
}

pub(super) fn orthogonal_segments_cross(
    first_start: &LayoutPoint,
    first_end: &LayoutPoint,
    second_start: &LayoutPoint,
    second_end: &LayoutPoint,
    epsilon: f64,
    endpoint_tolerance: f64,
) -> bool {
    let first_horizontal = same_y(first_start, first_end, epsilon);
    let first_vertical = same_x(first_start, first_end, epsilon);
    let second_horizontal = same_y(second_start, second_end, epsilon);
    let second_vertical = same_x(second_start, second_end, epsilon);
    if (first_horizontal && second_horizontal) || (first_vertical && second_vertical) {
        return false;
    }
    if !(first_horizontal || first_vertical) || !(second_horizontal || second_vertical) {
        return false;
    }

    let (horizontal_start, horizontal_end) = if first_horizontal {
        (first_start, first_end)
    } else {
        (second_start, second_end)
    };
    let (vertical_start, vertical_end) = if first_vertical {
        (first_start, first_end)
    } else {
        (second_start, second_end)
    };
    let horizontal_y = horizontal_start.y;
    let horizontal_min_x = horizontal_start.x.min(horizontal_end.x);
    let horizontal_max_x = horizontal_start.x.max(horizontal_end.x);
    let vertical_x = vertical_start.x;
    let vertical_min_y = vertical_start.y.min(vertical_end.y);
    let vertical_max_y = vertical_start.y.max(vertical_end.y);
    if vertical_x < horizontal_min_x
        || vertical_x > horizontal_max_x
        || horizontal_y < vertical_min_y
        || horizontal_y > vertical_max_y
    {
        return false;
    }

    let matches_horizontal_endpoint = [horizontal_start, horizontal_end].iter().any(|endpoint| {
        (vertical_x - endpoint.x).abs() < endpoint_tolerance
            && (horizontal_y - endpoint.y).abs() < endpoint_tolerance
    });
    let matches_vertical_endpoint = [vertical_start, vertical_end].iter().any(|endpoint| {
        (vertical_x - endpoint.x).abs() < endpoint_tolerance
            && (horizontal_y - endpoint.y).abs() < endpoint_tolerance
    });
    !(matches_horizontal_endpoint && matches_vertical_endpoint)
}

pub(super) fn same_axis_segments_overlap(
    first_start: &LayoutPoint,
    first_end: &LayoutPoint,
    second_start: &LayoutPoint,
    second_end: &LayoutPoint,
    epsilon: f64,
) -> bool {
    let first_horizontal = same_y(first_start, first_end, epsilon);
    let first_vertical = same_x(first_start, first_end, epsilon);
    let second_horizontal = same_y(second_start, second_end, epsilon);
    let second_vertical = same_x(second_start, second_end, epsilon);
    if first_vertical && second_vertical && same_x(first_start, second_start, epsilon) {
        return overlap_length(first_start.y, first_end.y, second_start.y, second_end.y) > epsilon;
    }
    if first_horizontal && second_horizontal && same_y(first_start, second_start, epsilon) {
        return overlap_length(first_start.x, first_end.x, second_start.x, second_end.x) > epsilon;
    }
    false
}

pub(super) fn segment_conflicts_with_any_edge(
    start: &LayoutPoint,
    end: &LayoutPoint,
    edges: &[WorkingEdge],
    excluded_edge_id: Option<&str>,
    epsilon: f64,
    skip_degenerate_other: bool,
) -> bool {
    edges.iter().any(|edge| {
        if excluded_edge_id.is_some_and(|excluded| edge.id == excluded) {
            return false;
        }
        edge.points.windows(2).any(|segment| {
            let other_start = &segment[0];
            let other_end = &segment[1];
            !(skip_degenerate_other && same_point(other_start, other_end, epsilon))
                && (orthogonal_segments_cross(start, end, other_start, other_end, epsilon, 1e-6)
                    || same_axis_segments_overlap(start, end, other_start, other_end, epsilon))
        })
    })
}

fn strictly_between(value: f64, first: f64, second: f64) -> bool {
    value > first.min(second) + EPSILON && value < first.max(second) - EPSILON
}

fn is_collinear_intermediate(
    previous: &LayoutPoint,
    current: &LayoutPoint,
    next: &LayoutPoint,
) -> bool {
    if same_x(previous, current, EPSILON) && same_x(current, next, EPSILON) {
        return strictly_between(current.y, previous.y, next.y);
    }
    if same_y(previous, current, EPSILON) && same_y(current, next, EPSILON) {
        return strictly_between(current.x, previous.x, next.x);
    }
    false
}

fn simplify_polyline_once(points: &[LayoutPoint]) -> (Vec<LayoutPoint>, bool) {
    let mut changed = false;
    let mut output = Vec::with_capacity(points.len());
    let mut index = 0;
    while index < points.len() {
        let current = &points[index];
        let next = points.get(index + 1);
        if let (Some(previous), Some(next)) = (output.last(), next) {
            if same_point(previous, next, EPSILON) {
                index += 2;
                changed = true;
                continue;
            }
            if is_collinear_intermediate(previous, current, next) {
                index += 1;
                changed = true;
                continue;
            }
        }
        output.push(current.clone());
        index += 1;
    }
    (output, changed)
}

pub(super) fn orthogonalize_polyline(points: &[LayoutPoint]) -> Vec<LayoutPoint> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut cleaned = vec![first.clone()];
    for current in &points[1..] {
        let previous = cleaned.last().expect("the first point is present");
        if !same_x(previous, current, EPSILON) && !same_y(previous, current, EPSILON) {
            let incoming_vertical = cleaned
                .get(cleaned.len().wrapping_sub(2))
                .is_some_and(|previous_previous| same_x(previous_previous, previous, EPSILON));
            let corner = if incoming_vertical {
                LayoutPoint {
                    x: previous.x,
                    y: current.y,
                }
            } else {
                LayoutPoint {
                    x: current.x,
                    y: previous.y,
                }
            };
            cleaned.push(corner);
        }
        cleaned.push(current.clone());
    }
    dedupe_consecutive_points(&cleaned, EPSILON)
}

pub(in crate::swimlane) fn simplify_polyline(points: &[LayoutPoint]) -> Vec<LayoutPoint> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut work = points.to_vec();
    for _ in 0..32 {
        let (next, changed) = simplify_polyline_once(&work);
        work = next;
        if !changed {
            break;
        }
    }
    work
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SwimlaneDirection;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
    }

    fn node(
        id: &str,
        kind: WorkingNodeKind,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> WorkingNode {
        WorkingNode {
            id: id.to_string(),
            label: id.to_string(),
            label_type: "text".to_string(),
            shape: "rect".to_string(),
            kind,
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

    fn edge(id: &str, from: &str, to: &str, points: Vec<LayoutPoint>) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            reference_id: id.to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points,
        }
    }

    fn layout(nodes: Vec<WorkingNode>, edges: Vec<WorkingEdge>) -> WorkingLayout {
        WorkingLayout {
            direction: SwimlaneDirection::Tb,
            nodes: nodes
                .into_iter()
                .map(|node| (node.id.clone(), node))
                .collect(),
            graph_edges: Vec::new(),
            original_edges: edges,
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
    fn orthogonalizes_a_diagonal_segment_with_an_l_bend() {
        let result = orthogonalize_polyline(&[point(0.0, 0.0), point(10.0, 10.0)]);
        assert_points(&result, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    }

    #[test]
    fn preserves_incoming_vertical_orientation_when_inserting_an_l_bend() {
        let result =
            orthogonalize_polyline(&[point(0.0, 0.0), point(0.0, 10.0), point(10.0, 20.0)]);
        assert_points(
            &result,
            &[(0.0, 0.0), (0.0, 10.0), (0.0, 20.0), (10.0, 20.0)],
        );
    }

    #[test]
    fn dedupes_consecutive_points_during_orthogonalization() {
        let result = orthogonalize_polyline(&[point(0.0, 0.0), point(0.0, 0.0), point(0.0, 10.0)]);
        assert_points(&result, &[(0.0, 0.0), (0.0, 10.0)]);
    }

    #[test]
    fn removes_out_and_back_spikes() {
        let result = simplify_polyline(&[
            point(0.0, 0.0),
            point(10.0, 0.0),
            point(0.0, 0.0),
            point(0.0, 10.0),
        ]);
        assert_points(&result, &[(0.0, 0.0), (0.0, 10.0)]);
    }

    #[test]
    fn removes_only_strictly_between_collinear_points() {
        let result = simplify_polyline(&[
            point(0.0, 0.0),
            point(5.0, 0.0),
            point(10.0, 0.0),
            point(10.0, 5.0),
        ]);
        assert_points(&result, &[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)]);

        let foldback = simplify_polyline(&[
            point(0.0, 0.0),
            point(10.0, 0.0),
            point(5.0, 0.0),
            point(5.0, 5.0),
        ]);
        assert_points(
            &foldback,
            &[(0.0, 0.0), (10.0, 0.0), (5.0, 0.0), (5.0, 5.0)],
        );
    }

    #[test]
    fn strict_crossing_excludes_t_junctions_and_endpoint_touches() {
        assert!(orthogonal_segments_strictly_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(0.0, -10.0),
            &point(0.0, 10.0),
            EPSILON,
        ));
        assert!(!orthogonal_segments_strictly_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 10.0),
            EPSILON,
        ));
        assert!(!orthogonal_segments_strictly_cross(
            &point(-10.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 10.0),
            EPSILON,
        ));
        assert!(!orthogonal_segments_strictly_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(-5.0, 0.0),
            &point(5.0, 0.0),
            EPSILON,
        ));
        assert!(!orthogonal_segments_strictly_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(9.9995, -10.0),
            &point(9.9995, 10.0),
            EPSILON,
        ));
    }

    #[test]
    fn crossing_counts_a_t_junction_but_not_a_shared_endpoint() {
        assert!(orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(0.0, -10.0),
            &point(0.0, 10.0),
            EPSILON,
            1e-6,
        ));
        assert!(orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 10.0),
            EPSILON,
            1e-6,
        ));
        assert!(!orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 0.0),
            &point(0.0, 10.0),
            EPSILON,
            1e-6,
        ));
        assert!(!orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.0),
            &point(-5.0, 0.0),
            &point(5.0, 0.0),
            EPSILON,
            1e-6,
        ));
        assert!(!orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(10.0, 10.0),
            &point(0.0, -10.0),
            &point(0.0, 10.0),
            EPSILON,
            1e-6,
        ));
        assert!(!orthogonal_segments_cross(
            &point(-10.0, 0.0),
            &point(10.0, 0.00001),
            &point(0.0, -10.0),
            &point(0.0, 10.0),
            1e-6,
            1e-6,
        ));
    }

    #[test]
    fn segment_rect_hits_honor_exclusions_and_shrink() {
        let rects = vec![RectEntry {
            id: "A".to_string(),
            rect: RectBounds {
                left: 0.0,
                right: 10.0,
                top: 0.0,
                bottom: 10.0,
            },
        }];
        assert!(segment_hits_any_rect(
            &point(-5.0, 5.0),
            &point(15.0, 5.0),
            &rects,
            &[],
            0.0,
        ));
        assert!(!segment_hits_any_rect(
            &point(-5.0, 5.0),
            &point(15.0, 5.0),
            &rects,
            &["A"],
            0.0,
        ));
        assert!(!segment_hits_any_rect(
            &point(-5.0, 0.5),
            &point(15.0, 0.5),
            &rects,
            &[],
            1.0,
        ));
    }

    #[test]
    fn collects_only_real_visible_nodes() {
        let layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 10.0, 20.0, 40.0, 20.0),
                node("group", WorkingNodeKind::Group, 10.0, 20.0, 40.0, 20.0),
                node("label", WorkingNodeKind::EdgeLabel, 10.0, 20.0, 40.0, 20.0),
                node("empty", WorkingNodeKind::Content, 10.0, 20.0, 0.0, 20.0),
            ],
            Vec::new(),
        );
        let (node_info_by_id, real_node_rects) = collect_real_node_bounds(&layout);

        assert_eq!(
            node_info_by_id
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["A"]
        );
        assert_eq!(
            real_node_rects,
            vec![RectEntry {
                id: "A".to_string(),
                rect: RectBounds {
                    left: -10.0,
                    right: 30.0,
                    top: 10.0,
                    bottom: 30.0,
                },
            }]
        );
    }

    #[test]
    fn classifies_upstream_three_segment_routes() {
        assert_eq!(
            classify_three_segment_route(
                &[
                    point(0.0, 0.0),
                    point(10.0, 0.0),
                    point(10.0, 20.0),
                    point(30.0, 20.0),
                ],
                EPSILON,
            )
            .map(|route| route.kind),
            Some(ThreeSegmentRouteKind::Hvh),
        );
        assert_eq!(
            classify_three_segment_route(
                &[
                    point(0.0, 0.0),
                    point(0.0, 10.0),
                    point(20.0, 10.0),
                    point(20.0, 30.0),
                ],
                EPSILON,
            )
            .map(|route| route.kind),
            Some(ThreeSegmentRouteKind::Vhv),
        );
        assert!(
            classify_three_segment_route(
                &[
                    point(0.0, 0.0),
                    point(10.0, 10.0),
                    point(20.0, 10.0),
                    point(20.0, 30.0),
                ],
                EPSILON,
            )
            .is_none()
        );
        assert!(same_axis_segments_overlap(
            &point(0.0, 0.0),
            &point(10.0, 0.0),
            &point(5.0, 0.0),
            &point(15.0, 0.0),
            EPSILON,
        ));
        assert!(!same_axis_segments_overlap(
            &point(0.0, 0.0),
            &point(10.0, 0.0),
            &point(10.0, 0.0),
            &point(15.0, 0.0),
            EPSILON,
        ));
        assert!(!same_axis_segments_overlap(
            &point(0.0, 0.0),
            &point(10.0, 0.0),
            &point(5.0, 1.0),
            &point(15.0, 1.0),
            EPSILON,
        ));
    }

    #[test]
    fn checks_candidate_segments_against_other_visible_edges() {
        let own_edge = edge("self", "A", "B", vec![point(0.0, 0.0), point(10.0, 0.0)]);
        let crossing = edge(
            "crossing",
            "C",
            "D",
            vec![point(5.0, -5.0), point(5.0, 5.0)],
        );
        let overlap = edge(
            "overlap",
            "E",
            "F",
            vec![point(20.0, 0.0), point(30.0, 0.0)],
        );

        assert!(segment_conflicts_with_any_edge(
            &point(0.0, 0.0),
            &point(10.0, 0.0),
            &[own_edge.clone(), crossing],
            Some("self"),
            EPSILON,
            false,
        ));
        assert!(segment_conflicts_with_any_edge(
            &point(21.0, 0.0),
            &point(25.0, 0.0),
            &[own_edge.clone(), overlap],
            Some("self"),
            EPSILON,
            false,
        ));
        assert!(!segment_conflicts_with_any_edge(
            &point(0.0, 0.0),
            &point(10.0, 0.0),
            &[own_edge],
            Some("self"),
            EPSILON,
            false,
        ));
    }

    #[test]
    fn preserves_upstream_orthogonal_segment_metadata_and_bend_counts() {
        let points = vec![
            point(0.0, 0.0),
            point(10.0, 0.0),
            point(10.0, 20.0),
            point(30.0, 20.0),
        ];
        let segments = orthogonal_segments_for_points(&points, EPSILON);

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].index, 0);
        assert!(segments[0].horizontal);
        assert!(!segments[0].vertical);
        assert_eq!(segments[1].index, 1);
        assert!(!segments[1].horizontal);
        assert!(segments[1].vertical);
        assert_eq!(count_orthogonal_bends(&points, EPSILON), 2);
        assert_eq!(
            same_axis_segment_overlap_length(
                &segments[0],
                &OrthogonalSegment {
                    index: 0,
                    a: point(5.0, 0.0),
                    b: point(15.0, 0.0),
                    horizontal: true,
                    vertical: false,
                },
                EPSILON,
            ),
            5.0
        );
        assert_eq!(overlap_length(0.0, 10.0, 20.0, 30.0), 0.0);
    }

    #[test]
    fn preserves_upstream_rectangle_and_layout_collection_semantics() {
        let inner = RectBounds::from_center(10.0, 20.0, 4.0, 8.0);
        let outer = inner.inflated(2.0);
        assert!(rect_contains_rect(outer, inner));
        assert!(rects_overlap(outer, inner));
        assert!(!rects_overlap(
            inner,
            RectBounds {
                left: inner.right,
                right: inner.right + 5.0,
                top: inner.top,
                bottom: inner.bottom,
            }
        ));

        let layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 10.0, 20.0, 40.0, 20.0),
                node("label", WorkingNodeKind::EdgeLabel, 30.0, 40.0, 10.0, 6.0),
                node("group", WorkingNodeKind::Group, 0.0, 0.0, 100.0, 100.0),
                node("zero", WorkingNodeKind::Content, 5.0, 6.0, 0.0, 0.0),
            ],
            Vec::new(),
        );
        assert_eq!(
            collect_layout_node_rects(&layout, true)
                .iter()
                .map(|rect| rect.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "label", "zero"]
        );
        assert_eq!(
            collect_layout_node_rects(&layout, false)
                .iter()
                .map(|rect| rect.node_id.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "zero"]
        );
    }

    #[test]
    fn builds_upstream_orthogonal_port_paths_for_all_axis_pairings() {
        assert_points(
            &build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Right,
                &point(20.0, 10.0),
                RectSide::Left,
                5.0,
                EPSILON,
            )
            .expect("opposing horizontal ports are routable"),
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (20.0, 10.0)],
        );
        assert_points(
            &build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Left,
                &point(10.0, 10.0),
                RectSide::Left,
                5.0,
                EPSILON,
            )
            .expect("same-side horizontal ports use an outside track"),
            &[(0.0, 0.0), (-5.0, 0.0), (-5.0, 10.0), (10.0, 10.0)],
        );
        assert_points(
            &build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Bottom,
                &point(10.0, 20.0),
                RectSide::Top,
                5.0,
                EPSILON,
            )
            .expect("opposing vertical ports are routable"),
            &[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 20.0)],
        );
        assert_points(
            &build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Right,
                &point(10.0, 10.0),
                RectSide::Top,
                5.0,
                EPSILON,
            )
            .expect("horizontal-to-vertical ports use one corner"),
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
        );
        assert_points(
            &build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Bottom,
                &point(10.0, 10.0),
                RectSide::Left,
                5.0,
                EPSILON,
            )
            .expect("vertical-to-horizontal ports use one corner"),
            &[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0)],
        );
        assert!(
            build_orthogonal_port_path(
                &point(0.0, 0.0),
                RectSide::Left,
                &point(10.0, 10.0),
                RectSide::Top,
                5.0,
                EPSILON,
            )
            .is_none()
        );
        assert_points(
            &build_same_side_track_path(
                &point(0.0, 0.0),
                RectSide::Bottom,
                &point(10.0, 10.0),
                20.0,
            ),
            &[(0.0, 0.0), (0.0, 20.0), (10.0, 20.0), (10.0, 10.0)],
        );
    }

    #[test]
    fn derives_ports_and_node_pair_geometry_from_measured_nodes() {
        let layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 0.0, 0.0, 20.0, 10.0),
                node("B", WorkingNodeKind::Content, 0.0, 40.0, 30.0, 20.0),
            ],
            Vec::new(),
        );
        let (node_info_by_id, _) = collect_real_node_bounds(&layout);
        let edge = edge("A_B", "A", "B", Vec::new());
        let pair = get_node_pair_geometry(&edge, &node_info_by_id, EPSILON)
            .expect("both endpoint nodes are measured");

        assert_eq!(pair.src_id, "A");
        assert_eq!(pair.dst_id, "B");
        assert!(pair.collinear_x);
        assert!(!pair.collinear_y);
        assert_points(
            &[
                port_for_rect_side(&pair.src_info, RectSide::Top),
                port_for_rect_side(&pair.src_info, RectSide::Bottom),
                port_for_rect_side(&pair.src_info, RectSide::Left),
                port_for_rect_side(&pair.src_info, RectSide::Right),
            ],
            &[(0.0, -5.0), (0.0, 5.0), (-10.0, 0.0), (10.0, 0.0)],
        );
        assert_eq!(pair.dst_info.cy, 40.0);
    }
}
