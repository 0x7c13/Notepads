use super::common::*;
use crate::Result;
use crate::swimlane::direction::LayoutWorkBudget;

const BUFFER: f64 = 2.0;
const CLEARANCE: f64 = 20.0;
const MAX_ITERATIONS: usize = 8;

fn middle_rail(points: &[LayoutPoint]) -> Option<OrthogonalSegment> {
    let segments = segments_for(points);
    if segments.len() != 3 {
        return None;
    }
    let middle = &segments[1];
    if segments[0].horizontal == middle.horizontal || segments[2].horizontal == middle.horizontal {
        return None;
    }
    Some(middle.clone())
}

fn blocking_rects<'a>(
    edge: &WorkingEdge,
    rail: &OrthogonalSegment,
    real_node_rects: &'a [RectEntry],
) -> Vec<&'a RectEntry> {
    real_node_rects
        .iter()
        .filter(|entry| entry.id != edge.from && entry.id != edge.to)
        .filter(|entry| {
            let rect = entry.rect;
            if rail.horizontal {
                overlap_length(rail.a.x, rail.b.x, rect.left, rect.right) >= MINIMUM_SHARED_LENGTH
                    && rail.a.y >= rect.top - BUFFER
                    && rail.a.y <= rect.bottom + BUFFER
            } else {
                overlap_length(rail.a.y, rail.b.y, rect.top, rect.bottom) >= MINIMUM_SHARED_LENGTH
                    && rail.a.x >= rect.left - BUFFER
                    && rail.a.x <= rect.right + BUFFER
            }
        })
        .collect()
}

fn candidate_by_moving_rail(
    points: &[LayoutPoint],
    rail: &OrthogonalSegment,
    coordinate: f64,
) -> Option<Vec<LayoutPoint>> {
    let mut candidate = points.to_vec();
    if rail.horizontal {
        candidate[rail.index].y = coordinate;
        candidate[rail.index + 1].y = coordinate;
    } else if rail.vertical {
        candidate[rail.index].x = coordinate;
        candidate[rail.index + 1].x = coordinate;
    } else {
        return None;
    }
    let simplified = simplify_polyline(&dedupe_consecutive_points(&candidate, EPSILON));
    (segments_for(&simplified).len() == simplified.len().saturating_sub(1)).then_some(simplified)
}

fn candidate_is_safe(
    edges: &[WorkingEdge],
    edge_index: usize,
    candidate: &[LayoutPoint],
    current_crossings: usize,
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let candidate_segments = segments_for(candidate);
    if candidate_segments.len() != candidate.len().saturating_sub(1) {
        return Ok(false);
    }
    work_budget.charge(
        candidate_segments
            .len()
            .saturating_mul(real_node_rects.len().saturating_add(label_rects.len())),
    )?;
    if path_hits_rects(
        &edges[edge_index],
        candidate,
        real_node_rects,
        label_rects,
        BUFFER,
    ) {
        return Ok(false);
    }
    work_budget.charge(edges.len().saturating_sub(1))?;
    for (other_index, other) in edges.iter().enumerate() {
        if other_index == edge_index {
            continue;
        }
        let other_segments = segments_for(&dedupe_consecutive_points(&other.points, EPSILON));
        if shared_track_conflicts(&candidate_segments, &other_segments) {
            return Ok(false);
        }
    }
    let replacements = ReplacementMap::from_iter([(edge_index, candidate.to_vec())]);
    Ok(checked_strict_crossing_count(edges, &replacements, work_budget)? <= current_crossings)
}

pub(in crate::swimlane::direction) fn lift_obstacle_hugging_same_side_rails(
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
        let mut replacement = None;
        work_budget.charge(layout.original_edges.len())?;
        for (edge_index, edge) in layout.original_edges.iter().enumerate() {
            let points = dedupe_consecutive_points(&edge.points, EPSILON);
            let Some(rail) = middle_rail(&points) else {
                continue;
            };
            work_budget.charge(real_node_rects.len())?;
            let blockers = blocking_rects(edge, &rail, &real_node_rects);
            if blockers.is_empty() {
                continue;
            }
            work_budget.charge(blockers.len().saturating_mul(2))?;
            let coordinates = if rail.horizontal {
                [
                    blockers
                        .iter()
                        .map(|entry| entry.rect.top)
                        .fold(f64::INFINITY, f64::min)
                        - CLEARANCE,
                    blockers
                        .iter()
                        .map(|entry| entry.rect.bottom)
                        .fold(f64::NEG_INFINITY, f64::max)
                        + CLEARANCE,
                ]
            } else {
                [
                    blockers
                        .iter()
                        .map(|entry| entry.rect.left)
                        .fold(f64::INFINITY, f64::min)
                        - CLEARANCE,
                    blockers
                        .iter()
                        .map(|entry| entry.rect.right)
                        .fold(f64::NEG_INFINITY, f64::max)
                        + CLEARANCE,
                ]
            };
            for coordinate in coordinates {
                work_budget.charge(points.len())?;
                let Some(candidate) = candidate_by_moving_rail(&points, &rail, coordinate) else {
                    continue;
                };
                work_budget.charge(1)?;
                if candidate_is_safe(
                    &layout.original_edges,
                    edge_index,
                    &candidate,
                    current_crossings,
                    &real_node_rects,
                    &label_rects,
                    work_budget,
                )? {
                    replacement = Some((edge_index, candidate));
                    break;
                }
            }
            if replacement.is_some() {
                break;
            }
        }
        let Some((edge_index, points)) = replacement else {
            return Ok(());
        };
        layout.original_edges[edge_index].points = points;
    }
    Ok(())
}
