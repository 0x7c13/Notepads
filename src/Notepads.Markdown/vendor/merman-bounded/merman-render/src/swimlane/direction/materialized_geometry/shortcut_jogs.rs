use super::common::*;
use crate::Result;
use crate::swimlane::direction::LayoutWorkBudget;
use indexmap::IndexSet;

const BUFFER: f64 = 2.0;
const MAX_ITERATIONS: usize = 8;

fn segment_runs_along_rect_border(segment: &OrthogonalSegment, rect: RectBounds) -> bool {
    if segment.horizontal {
        let on_border =
            (segment.a.y - rect.top).abs() < 1.0 || (segment.a.y - rect.bottom).abs() < 1.0;
        return on_border
            && overlap_length(segment.a.x, segment.b.x, rect.left, rect.right)
                >= MINIMUM_SHARED_LENGTH;
    }
    if segment.vertical {
        let on_border =
            (segment.a.x - rect.left).abs() < 1.0 || (segment.a.x - rect.right).abs() < 1.0;
        return on_border
            && overlap_length(segment.a.y, segment.b.y, rect.top, rect.bottom)
                >= MINIMUM_SHARED_LENGTH;
    }
    false
}

fn endpoint_rects_for(layout: &WorkingLayout, edge: &WorkingEdge) -> Vec<RectBounds> {
    [&edge.from, &edge.to]
        .into_iter()
        .filter_map(|id| layout.nodes.get(id))
        .filter_map(rect_of_node_bounds)
        .collect()
}

fn shortcut_candidates_at(points: &[LayoutPoint], index: usize) -> Vec<Vec<LayoutPoint>> {
    if index + 3 >= points.len() {
        return Vec::new();
    }
    let p0 = &points[index];
    let p1 = &points[index + 1];
    let p2 = &points[index + 2];
    let p3 = &points[index + 3];
    let horizontal_vertical_horizontal = is_horizontal_segment(p0, p1, EPSILON)
        && is_vertical_segment(p1, p2, EPSILON)
        && is_horizontal_segment(p2, p3, EPSILON);
    let vertical_horizontal_vertical = is_vertical_segment(p0, p1, EPSILON)
        && is_horizontal_segment(p1, p2, EPSILON)
        && is_vertical_segment(p2, p3, EPSILON);
    if !horizontal_vertical_horizontal && !vertical_horizontal_vertical {
        return Vec::new();
    }
    let outer_segments_oppose = if horizontal_vertical_horizontal {
        (p1.x - p0.x).signum() != (p3.x - p2.x).signum()
    } else {
        (p1.y - p0.y).signum() != (p3.y - p2.y).signum()
    };
    if !outer_segments_oppose {
        return Vec::new();
    }

    let raw_candidates = if same_x(p0, p3, EPSILON) || same_y(p0, p3, EPSILON) {
        let mut candidate = points[..=index].to_vec();
        candidate.extend_from_slice(&points[index + 3..]);
        vec![candidate]
    } else {
        [
            LayoutPoint { x: p0.x, y: p3.y },
            LayoutPoint { x: p3.x, y: p0.y },
        ]
        .into_iter()
        .map(|corner| {
            let mut candidate = points[..=index].to_vec();
            candidate.push(corner);
            candidate.extend_from_slice(&points[index + 3..]);
            candidate
        })
        .collect()
    };

    let mut seen = IndexSet::new();
    raw_candidates
        .into_iter()
        .map(|candidate| simplify_polyline(&dedupe_consecutive_points(&candidate, EPSILON)))
        .filter(|candidate| {
            segments_for(candidate).len() == candidate.len().saturating_sub(1)
                && candidate.iter().any(|point| same_point(point, p3, EPSILON))
                && seen.insert(candidate_key(candidate))
        })
        .collect()
}

fn candidate_is_safe(
    layout: &WorkingLayout,
    edge_index: usize,
    candidate: &[LayoutPoint],
    current_crossings: usize,
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let edge = &layout.original_edges[edge_index];
    let endpoint_rects = endpoint_rects_for(layout, edge);
    let candidate_segments = segments_for(candidate);
    let excluded = endpoint_id_slices(edge);
    work_budget.charge(
        candidate_segments.len().saturating_mul(
            real_node_rects
                .len()
                .saturating_add(label_rects.len())
                .saturating_add(endpoint_rects.len()),
        ),
    )?;
    for segment in &candidate_segments {
        if segment_hits_any_rect(&segment.a, &segment.b, real_node_rects, &excluded, -BUFFER)
            || segment_hits_any_rect(&segment.a, &segment.b, label_rects, &[], -BUFFER)
            || endpoint_rects
                .iter()
                .any(|rect| segment_runs_along_rect_border(segment, *rect))
        {
            return Ok(false);
        }
    }
    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
    for (other_index, other) in layout.original_edges.iter().enumerate() {
        if other_index == edge_index {
            continue;
        }
        let other_segments = segments_for(&dedupe_consecutive_points(&other.points, EPSILON));
        if shared_track_conflicts(&candidate_segments, &other_segments) {
            return Ok(false);
        }
    }
    let replacements = ReplacementMap::from_iter([(edge_index, candidate.to_vec())]);
    Ok(
        checked_strict_crossing_count(&layout.original_edges, &replacements, work_budget)?
            <= current_crossings,
    )
}

pub(in crate::swimlane::direction) fn shortcut_redundant_orthogonal_jogs(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, label_rects) = collect_node_rect_entries(layout);
    for _ in 0..MAX_ITERATIONS {
        let current_crossings = checked_strict_crossing_count(
            &layout.original_edges,
            &ReplacementMap::new(),
            work_budget,
        )?;
        let mut best_edge = None;
        let mut best_path = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = usize::MAX;
        let mut best_length = f64::INFINITY;

        work_budget.charge(layout.original_edges.len())?;
        for (edge_index, edge) in layout.original_edges.iter().enumerate() {
            let current_points = dedupe_consecutive_points(&edge.points, EPSILON);
            let current_bends = count_orthogonal_bends(&current_points, EPSILON);
            let current_length = euclidean_path_length(&current_points);
            if current_points.len() < 4 {
                continue;
            }
            for index in 0..=current_points.len() - 4 {
                for candidate in shortcut_candidates_at(&current_points, index) {
                    work_budget.charge(1)?;
                    let candidate_bends = count_orthogonal_bends(&candidate, EPSILON);
                    let candidate_length = euclidean_path_length(&candidate);
                    let improves_shape = candidate_bends < current_bends
                        || (candidate_bends == current_bends
                            && candidate_length < current_length - EPSILON);
                    if !improves_shape {
                        continue;
                    }
                    if !candidate_is_safe(
                        layout,
                        edge_index,
                        &candidate,
                        current_crossings,
                        &real_node_rects,
                        &label_rects,
                        work_budget,
                    )? {
                        continue;
                    }
                    let replacements = ReplacementMap::from_iter([(edge_index, candidate.clone())]);
                    let candidate_crossings = checked_strict_crossing_count(
                        &layout.original_edges,
                        &replacements,
                        work_budget,
                    )?;
                    if candidate_crossings > best_crossings
                        || (candidate_crossings == best_crossings
                            && (candidate_bends > best_bends
                                || (candidate_bends == best_bends
                                    && candidate_length >= best_length)))
                    {
                        continue;
                    }
                    best_edge = Some(edge_index);
                    best_path = Some(candidate);
                    best_crossings = candidate_crossings;
                    best_bends = candidate_bends;
                    best_length = candidate_length;
                }
            }
        }
        let (Some(edge_index), Some(points)) = (best_edge, best_path) else {
            return Ok(());
        };
        layout.original_edges[edge_index].points = points;
    }
    Ok(())
}
