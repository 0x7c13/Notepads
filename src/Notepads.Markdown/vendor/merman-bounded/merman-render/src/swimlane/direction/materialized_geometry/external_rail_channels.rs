use super::common::*;
use crate::Result;
use crate::swimlane::direction::{LayoutWorkBudget, unordered_pair_count};
use indexmap::IndexMap;

const BUFFER: f64 = 2.0;
const RAIL_CHANNEL_GAP: f64 = 12.0;
const MAX_ITERATIONS: usize = 4;
const MAX_EXHAUSTIVE_COMPONENT: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
struct ExternalRail {
    edge_index: usize,
    points: Vec<LayoutPoint>,
    segment_index: usize,
    axis: RailAxis,
    side: RectSide,
    coordinate: f64,
    minimum: f64,
    maximum: f64,
}

fn endpoint_rects_for(
    layout: &WorkingLayout,
    edge: &WorkingEdge,
) -> Option<(RectBounds, RectBounds)> {
    let source = rect_of_node_bounds(layout.nodes.get(&edge.from)?)?;
    let destination = rect_of_node_bounds(layout.nodes.get(&edge.to)?)?;
    Some((source, destination))
}

fn external_rail_for_segment(
    layout: &WorkingLayout,
    edge_index: usize,
    points: &[LayoutPoint],
    segment: &OrthogonalSegment,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<ExternalRail>> {
    if segment.index == 0 || segment.index + 1 >= points.len().saturating_sub(1) {
        return Ok(None);
    }
    let Some((source, destination)) =
        endpoint_rects_for(layout, &layout.original_edges[edge_index])
    else {
        return Ok(None);
    };
    if segment.vertical {
        let coordinate = segment.a.x;
        let left_bound = source.left.min(destination.left);
        let right_bound = source.right.max(destination.right);
        let side = if coordinate < left_bound - EPSILON {
            RectSide::Left
        } else if coordinate > right_bound + EPSILON {
            RectSide::Right
        } else {
            return Ok(None);
        };
        work_budget.charge(points.len())?;
        return Ok(Some(ExternalRail {
            edge_index,
            points: points.to_vec(),
            segment_index: segment.index,
            axis: RailAxis::Vertical,
            side,
            coordinate,
            minimum: segment.a.y.min(segment.b.y),
            maximum: segment.a.y.max(segment.b.y),
        }));
    }
    if segment.horizontal {
        let coordinate = segment.a.y;
        let top_bound = source.top.min(destination.top);
        let bottom_bound = source.bottom.max(destination.bottom);
        let side = if coordinate < top_bound - EPSILON {
            RectSide::Top
        } else if coordinate > bottom_bound + EPSILON {
            RectSide::Bottom
        } else {
            return Ok(None);
        };
        work_budget.charge(points.len())?;
        return Ok(Some(ExternalRail {
            edge_index,
            points: points.to_vec(),
            segment_index: segment.index,
            axis: RailAxis::Horizontal,
            side,
            coordinate,
            minimum: segment.a.x.min(segment.b.x),
            maximum: segment.a.x.max(segment.b.x),
        }));
    }
    Ok(None)
}

fn collect_external_rails(
    layout: &WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<ExternalRail>> {
    let mut rails = Vec::new();
    for (edge_index, edge) in layout.original_edges.iter().enumerate() {
        let points = dedupe_consecutive_points(&edge.points, EPSILON);
        for segment in segments_for(&points) {
            if let Some(rail) =
                external_rail_for_segment(layout, edge_index, &points, &segment, work_budget)?
            {
                rails.push(rail);
            }
        }
    }
    Ok(rails)
}

fn rails_interact(first: &ExternalRail, second: &ExternalRail) -> bool {
    first.edge_index != second.edge_index
        && first.axis == second.axis
        && first.side == second.side
        && overlap_length(first.minimum, first.maximum, second.minimum, second.maximum)
            >= MINIMUM_SHARED_LENGTH
}

fn connected_components(
    rails: &[ExternalRail],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<Vec<ExternalRail>>> {
    let mut result = Vec::new();
    let mut seen = vec![false; rails.len()];
    for rail_index in 0..rails.len() {
        if seen[rail_index] {
            continue;
        }
        let mut queue = vec![rail_index];
        let mut component = Vec::new();
        seen[rail_index] = true;
        while let Some(current_index) = queue.pop() {
            work_budget.charge(rails[current_index].points.len())?;
            component.push(rails[current_index].clone());
            for next_index in 0..rails.len() {
                if !seen[next_index] && rails_interact(&rails[current_index], &rails[next_index]) {
                    seen[next_index] = true;
                    queue.push(next_index);
                }
            }
        }
        if component.len() > 1 {
            result.push(component);
        }
    }
    Ok(result)
}

fn unique_coordinates_for(
    component: &[ExternalRail],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<f64>> {
    let mut coordinates = Vec::new();
    for rail in component {
        work_budget.charge(coordinates.len())?;
        if !coordinates
            .iter()
            .any(|coordinate: &f64| (*coordinate - rail.coordinate).abs() < EPSILON)
        {
            coordinates.push(rail.coordinate);
        }
    }
    while coordinates.len() < component.len() {
        work_budget.charge(coordinates.len().saturating_mul(2))?;
        let minimum = coordinates.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = coordinates
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let distance = RAIL_CHANNEL_GAP * (component.len() - coordinates.len()) as f64;
        coordinates.push(
            if matches!(component[0].side, RectSide::Left | RectSide::Top) {
                minimum - distance
            } else {
                maximum + distance
            },
        );
    }
    Ok(coordinates)
}

fn visit_permutations(
    coordinates: &[f64],
    current: &[f64],
    used: &mut [bool],
    next: &mut Vec<f64>,
    assignments: &mut Vec<Vec<f64>>,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    if next.len() == current.len() {
        work_budget.charge(current.len())?;
        if next
            .iter()
            .zip(current)
            .any(|(candidate, existing)| (candidate - existing).abs() >= EPSILON)
        {
            work_budget.charge(1usize.saturating_add(next.len()))?;
            assignments.push(next.clone());
        }
        return Ok(());
    }
    for (coordinate_index, coordinate) in coordinates.iter().copied().enumerate() {
        if used[coordinate_index] {
            continue;
        }
        used[coordinate_index] = true;
        next.push(coordinate);
        visit_permutations(coordinates, current, used, next, assignments, work_budget)?;
        next.pop();
        used[coordinate_index] = false;
    }
    Ok(())
}

fn coordinate_assignments_for(
    component: &[ExternalRail],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<Vec<f64>>> {
    work_budget.charge(component.len())?;
    let current: Vec<_> = component.iter().map(|rail| rail.coordinate).collect();
    let coordinates = unique_coordinates_for(component, work_budget)?;
    let mut assignments = Vec::new();
    if component.len() <= MAX_EXHAUSTIVE_COMPONENT {
        visit_permutations(
            &coordinates,
            &current,
            &mut vec![false; coordinates.len()],
            &mut Vec::new(),
            &mut assignments,
            work_budget,
        )?;
        return Ok(assignments);
    }
    for first in 0..current.len() {
        for second in first + 1..current.len() {
            work_budget.charge(1usize.saturating_add(current.len()))?;
            let mut assignment = current.clone();
            assignment.swap(first, second);
            assignments.push(assignment);
        }
    }
    Ok(assignments)
}

fn replacements_for_assignment(
    component: &[ExternalRail],
    assignment: &[f64],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<ReplacementMap>> {
    work_budget.charge(component.len())?;
    let mut drafts: IndexMap<usize, Vec<LayoutPoint>> = IndexMap::new();
    for (index, rail) in component.iter().enumerate() {
        if !drafts.contains_key(&rail.edge_index) {
            work_budget.charge(rail.points.len())?;
            drafts.insert(rail.edge_index, rail.points.clone());
        }
        let points = drafts
            .get_mut(&rail.edge_index)
            .expect("the rail draft was inserted above");
        if rail.axis == RailAxis::Vertical {
            points[rail.segment_index].x = assignment[index];
            points[rail.segment_index + 1].x = assignment[index];
        } else {
            points[rail.segment_index].y = assignment[index];
            points[rail.segment_index + 1].y = assignment[index];
        }
    }
    work_budget.charge(drafts.len())?;
    let mut replacements = ReplacementMap::new();
    for (edge_index, points) in drafts {
        work_budget.charge(points.len())?;
        let simplified = simplify_polyline(&dedupe_consecutive_points(&points, EPSILON));
        if segments_for(&simplified).len() != simplified.len().saturating_sub(1) {
            return Ok(None);
        }
        replacements.insert(edge_index, simplified);
    }
    Ok(Some(replacements))
}

fn candidate_is_safe(
    layout: &WorkingLayout,
    replacements: &ReplacementMap,
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    for (edge_index, points) in replacements {
        work_budget.charge(
            points
                .len()
                .saturating_sub(1)
                .saturating_mul(real_node_rects.len().saturating_add(label_rects.len())),
        )?;
        if path_hits_rects(
            &layout.original_edges[*edge_index],
            points,
            real_node_rects,
            label_rects,
            BUFFER,
        ) {
            return Ok(false);
        }
    }
    work_budget.charge(unordered_pair_count(layout.original_edges.len()))?;
    for first_index in 0..layout.original_edges.len() {
        let first_changed = replacements.contains_key(&first_index);
        let first_segments = segments_for(&points_for(
            &layout.original_edges,
            first_index,
            replacements,
        ));
        for second_index in first_index + 1..layout.original_edges.len() {
            if !first_changed && !replacements.contains_key(&second_index) {
                continue;
            }
            let second_segments = segments_for(&points_for(
                &layout.original_edges,
                second_index,
                replacements,
            ));
            if shared_track_conflicts(&first_segments, &second_segments) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(in crate::swimlane::direction) fn reassign_crossing_external_rail_channels(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, label_rects) = collect_node_rect_entries(layout);
    for _ in 0..MAX_ITERATIONS {
        let empty = ReplacementMap::new();
        let current_crossings =
            checked_strict_crossing_count(&layout.original_edges, &empty, work_budget)?;
        if current_crossings == 0 {
            return Ok(());
        }
        let mut best_replacements = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = checked_total_bends(&layout.original_edges, &empty, work_budget)?;
        let mut best_displacement = f64::INFINITY;

        work_budget.charge(layout.original_edges.len())?;
        let edge_point_work = layout.original_edges.iter().fold(0usize, |total, edge| {
            total.saturating_add(edge.points.len())
        });
        work_budget.charge(edge_point_work)?;
        let external_rails = collect_external_rails(layout, work_budget)?;
        work_budget.charge(external_rails.len().saturating_mul(external_rails.len()))?;
        for component in connected_components(&external_rails, work_budget)? {
            for assignment in coordinate_assignments_for(&component, work_budget)? {
                let Some(replacements) =
                    replacements_for_assignment(&component, &assignment, work_budget)?
                else {
                    continue;
                };
                if !candidate_is_safe(
                    layout,
                    &replacements,
                    &real_node_rects,
                    &label_rects,
                    work_budget,
                )? {
                    continue;
                }
                let candidate_crossings = checked_strict_crossing_count(
                    &layout.original_edges,
                    &replacements,
                    work_budget,
                )?;
                if candidate_crossings >= current_crossings {
                    continue;
                }
                let candidate_bends =
                    checked_total_bends(&layout.original_edges, &replacements, work_budget)?;
                work_budget.charge(component.len())?;
                let candidate_displacement: f64 = component
                    .iter()
                    .zip(&assignment)
                    .map(|(rail, coordinate)| (coordinate - rail.coordinate).abs())
                    .sum();
                if candidate_crossings > best_crossings
                    || (candidate_crossings == best_crossings
                        && (candidate_bends > best_bends
                            || (candidate_bends == best_bends
                                && candidate_displacement >= best_displacement)))
                {
                    continue;
                }
                best_replacements = Some(replacements);
                best_crossings = candidate_crossings;
                best_bends = candidate_bends;
                best_displacement = candidate_displacement;
            }
        }
        let Some(replacements) = best_replacements else {
            return Ok(());
        };
        apply_replacements(&mut layout.original_edges, replacements);
    }
    Ok(())
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};

    fn rail(edge_index: usize, coordinate: f64) -> ExternalRail {
        ExternalRail {
            edge_index,
            points: vec![
                LayoutPoint {
                    x: coordinate,
                    y: 0.0,
                },
                LayoutPoint {
                    x: coordinate,
                    y: 10.0,
                },
            ],
            segment_index: 0,
            axis: RailAxis::Vertical,
            side: RectSide::Left,
            coordinate,
            minimum: 0.0,
            maximum: 10.0,
        }
    }

    #[test]
    fn permutation_is_rejected_before_the_assignment_clone_is_materialized() {
        let component = vec![rail(0, 0.0), rail(1, 1.0), rail(2, 2.0)];
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 12)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error = coordinate_assignments_for(&component, &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 16);
        assert_eq!(error.max, 12);
        assert_eq!(budget.used(), 12);
    }
}
