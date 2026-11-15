use crate::Result;
pub(super) use crate::model::LayoutPoint;
pub(super) use crate::swimlane::direction::geometry::{
    EPSILON, NodeBoundsInfo, OrthogonalSegment, RectBounds, RectEntry, RectSide,
    build_orthogonal_port_path, build_same_side_track_path, collect_node_rect_entries,
    collect_real_node_bounds, count_orthogonal_bends, dedupe_consecutive_points,
    is_horizontal_segment, is_vertical_segment, orthogonal_segments_for_points,
    orthogonal_segments_strictly_cross, overlap_length, port_for_rect_side, rect_of_node_bounds,
    same_axis_segment_overlap_length, same_point, same_x, same_y, segment_hits_any_rect,
    simplify_polyline,
};
use crate::swimlane::direction::{LayoutWorkBudget, unordered_pair_count};
pub(super) use crate::swimlane::working::{
    WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind,
};
use indexmap::{IndexMap, IndexSet};

pub(super) const MINIMUM_SHARED_LENGTH: f64 = 8.0;
pub(super) type ReplacementMap = IndexMap<usize, Vec<LayoutPoint>>;

pub(super) fn points_for(
    edges: &[WorkingEdge],
    edge_index: usize,
    replacements: &ReplacementMap,
) -> Vec<LayoutPoint> {
    dedupe_consecutive_points(
        replacements
            .get(&edge_index)
            .unwrap_or(&edges[edge_index].points),
        EPSILON,
    )
}

pub(super) fn segments_for(points: &[LayoutPoint]) -> Vec<OrthogonalSegment> {
    orthogonal_segments_for_points(points, EPSILON)
}

pub(super) fn crossing_count_between_segments(
    first: &[OrthogonalSegment],
    second: &[OrthogonalSegment],
) -> usize {
    first
        .iter()
        .map(|first_segment| {
            second
                .iter()
                .filter(|second_segment| {
                    orthogonal_segments_strictly_cross(
                        &first_segment.a,
                        &first_segment.b,
                        &second_segment.a,
                        &second_segment.b,
                        EPSILON,
                    )
                })
                .count()
        })
        .sum()
}

pub(super) fn crossing_count_between_paths(first: &[LayoutPoint], second: &[LayoutPoint]) -> usize {
    crossing_count_between_segments(&segments_for(first), &segments_for(second))
}

pub(super) fn strict_crossing_count(edges: &[WorkingEdge], replacements: &ReplacementMap) -> usize {
    let mut count = 0;
    for first_index in 0..edges.len() {
        let first = segments_for(&points_for(edges, first_index, replacements));
        for second_index in first_index + 1..edges.len() {
            let second = segments_for(&points_for(edges, second_index, replacements));
            count += crossing_count_between_segments(&first, &second);
        }
    }
    count
}

pub(super) fn checked_strict_crossing_count(
    edges: &[WorkingEdge],
    replacements: &ReplacementMap,
    work_budget: &mut LayoutWorkBudget,
) -> Result<usize> {
    work_budget.charge(unordered_pair_count(edges.len()))?;
    Ok(strict_crossing_count(edges, replacements))
}

pub(super) fn total_bends(edges: &[WorkingEdge], replacements: &ReplacementMap) -> usize {
    (0..edges.len())
        .map(|edge_index| {
            count_orthogonal_bends(&points_for(edges, edge_index, replacements), EPSILON)
        })
        .sum()
}

pub(super) fn checked_total_bends(
    edges: &[WorkingEdge],
    replacements: &ReplacementMap,
    work_budget: &mut LayoutWorkBudget,
) -> Result<usize> {
    work_budget.charge(edges.len())?;
    Ok(total_bends(edges, replacements))
}

pub(super) fn euclidean_path_length(points: &[LayoutPoint]) -> f64 {
    segments_for(points)
        .iter()
        .map(|segment| (segment.a.x - segment.b.x).hypot(segment.a.y - segment.b.y))
        .sum()
}

pub(super) fn manhattan_path_length(points: &[LayoutPoint]) -> f64 {
    points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).abs() + (pair[1].y - pair[0].y).abs())
        .sum()
}

pub(super) fn endpoint_id_slices(edge: &WorkingEdge) -> [&str; 2] {
    [edge.from.as_str(), edge.to.as_str()]
}

pub(super) fn path_hits_rects(
    edge: &WorkingEdge,
    path: &[LayoutPoint],
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    buffer: f64,
) -> bool {
    let excluded = endpoint_id_slices(edge);
    segments_for(path).iter().any(|segment| {
        segment_hits_any_rect(&segment.a, &segment.b, real_node_rects, &excluded, -buffer)
            || segment_hits_any_rect(&segment.a, &segment.b, label_rects, &[], -buffer)
    })
}

pub(super) fn shared_track_conflicts(
    candidate_segments: &[OrthogonalSegment],
    other_segments: &[OrthogonalSegment],
) -> bool {
    candidate_segments.iter().any(|candidate| {
        other_segments.iter().any(|other| {
            same_axis_segment_overlap_length(candidate, other, 0.5) >= MINIMUM_SHARED_LENGTH
        })
    })
}

pub(super) fn apply_replacements(edges: &mut [WorkingEdge], replacements: ReplacementMap) {
    for (edge_index, points) in replacements {
        edges[edge_index].points = points;
    }
}

pub(super) fn push_orthogonal_candidate(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    points: Vec<LayoutPoint>,
) {
    let candidate = simplify_polyline(&dedupe_consecutive_points(&points, EPSILON));
    if segments_for(&candidate).len() == candidate.len().saturating_sub(1) {
        candidates.push(candidate);
    }
}

pub(super) fn candidate_key(points: &[LayoutPoint]) -> String {
    points
        .iter()
        .map(|point| format!("{:.3},{:.3}", point.x, point.y))
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn dedupe_candidate_paths(
    candidates: impl IntoIterator<Item = Vec<LayoutPoint>>,
) -> Vec<Vec<LayoutPoint>> {
    let mut seen = IndexSet::new();
    candidates
        .into_iter()
        .map(|candidate| dedupe_consecutive_points(&candidate, EPSILON))
        .filter(|candidate| candidate.len() >= 2 && seen.insert(candidate_key(candidate)))
        .collect()
}
