use super::common::*;
use crate::Result;
use crate::swimlane::direction::LayoutWorkBudget;
use indexmap::IndexMap;

const BUFFER: f64 = 2.0;
const MAX_ITERATIONS: usize = 4;

#[derive(Clone)]
struct TerminalTail {
    tail_start: LayoutPoint,
    terminal: LayoutPoint,
}

fn terminal_tail_for(edges: &[WorkingEdge], edge_index: usize) -> Option<TerminalTail> {
    let points = dedupe_consecutive_points(&edges[edge_index].points, EPSILON);
    if points.len() < 4 {
        return None;
    }
    let tail_start = &points[points.len() - 2];
    let terminal = points.last()?;
    (is_horizontal_segment(tail_start, terminal, EPSILON)
        || is_vertical_segment(tail_start, terminal, EPSILON))
    .then(|| TerminalTail {
        tail_start: tail_start.clone(),
        terminal: terminal.clone(),
    })
}

fn candidate_with_destination_tail(
    edges: &[WorkingEdge],
    edge_index: usize,
    tail: &TerminalTail,
) -> Option<Vec<LayoutPoint>> {
    let points = dedupe_consecutive_points(&edges[edge_index].points, EPSILON);
    if points.len() < 3 {
        return None;
    }
    let start = &points[0];
    let first_turn = &points[1];
    let connector = if is_horizontal_segment(start, first_turn, EPSILON) {
        LayoutPoint {
            x: first_turn.x,
            y: tail.tail_start.y,
        }
    } else if is_vertical_segment(start, first_turn, EPSILON) {
        LayoutPoint {
            x: tail.tail_start.x,
            y: first_turn.y,
        }
    } else {
        return None;
    };
    let candidate = simplify_polyline(&dedupe_consecutive_points(
        &[
            start.clone(),
            first_turn.clone(),
            connector,
            tail.tail_start.clone(),
            tail.terminal.clone(),
        ],
        EPSILON,
    ));
    (segments_for(&candidate).len() == candidate.len().saturating_sub(1)).then_some(candidate)
}

fn path_has_node_hit(
    edge: &WorkingEdge,
    path: &[LayoutPoint],
    real_node_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let excluded = endpoint_id_slices(edge);
    let segments = segments_for(path);
    work_budget.charge(segments.len().saturating_mul(real_node_rects.len()))?;
    Ok(segments.iter().any(|segment| {
        segment_hits_any_rect(&segment.a, &segment.b, real_node_rects, &excluded, -BUFFER)
    }))
}

fn path_has_shared_track(
    edges: &[WorkingEdge],
    edge_index: usize,
    path: &[LayoutPoint],
    replacements: &ReplacementMap,
) -> bool {
    let candidate_segments = segments_for(path);
    (0..edges.len()).any(|other_index| {
        other_index != edge_index
            && shared_track_conflicts(
                &candidate_segments,
                &segments_for(&points_for(edges, other_index, replacements)),
            )
    })
}

fn edges_by_destination(layout: &WorkingLayout) -> IndexMap<String, Vec<usize>> {
    let mut result: IndexMap<String, Vec<usize>> = IndexMap::new();
    for (edge_index, edge) in layout.original_edges.iter().enumerate() {
        if !layout.nodes.contains_key(&edge.to)
            || dedupe_consecutive_points(&edge.points, EPSILON).len() < 4
        {
            continue;
        }
        result.entry(edge.to.clone()).or_default().push(edge_index);
    }
    result
}

pub(in crate::swimlane::direction) fn swap_destination_terminal_tails_to_reduce_crossings(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, _) = collect_node_rect_entries(layout);
    for _ in 0..MAX_ITERATIONS {
        let empty = ReplacementMap::new();
        let current_crossings =
            checked_strict_crossing_count(&layout.original_edges, &empty, work_budget)?;
        if current_crossings == 0 {
            return Ok(());
        }
        work_budget.charge(layout.original_edges.len())?;
        let edges_by_destination = edges_by_destination(layout);
        let current_bends = checked_total_bends(&layout.original_edges, &empty, work_budget)?;
        let mut best_replacements = None;
        let mut best_crossings = current_crossings;
        let mut best_bends = current_bends;

        for destination_edges in edges_by_destination.values() {
            for first_position in 0..destination_edges.len() {
                for second_position in first_position + 1..destination_edges.len() {
                    work_budget.charge(1)?;
                    let first_index = destination_edges[first_position];
                    let second_index = destination_edges[second_position];
                    let candidate_path_work = layout.original_edges[first_index]
                        .points
                        .len()
                        .saturating_add(layout.original_edges[second_index].points.len());
                    work_budget.charge(candidate_path_work)?;
                    let (Some(first_tail), Some(second_tail)) = (
                        terminal_tail_for(&layout.original_edges, first_index),
                        terminal_tail_for(&layout.original_edges, second_index),
                    ) else {
                        continue;
                    };
                    work_budget.charge(candidate_path_work)?;
                    let (Some(first_candidate), Some(second_candidate)) = (
                        candidate_with_destination_tail(
                            &layout.original_edges,
                            first_index,
                            &second_tail,
                        ),
                        candidate_with_destination_tail(
                            &layout.original_edges,
                            second_index,
                            &first_tail,
                        ),
                    ) else {
                        continue;
                    };
                    let replacements = ReplacementMap::from_iter([
                        (first_index, first_candidate.clone()),
                        (second_index, second_candidate.clone()),
                    ]);
                    if path_has_node_hit(
                        &layout.original_edges[first_index],
                        &first_candidate,
                        &real_node_rects,
                        work_budget,
                    )? {
                        continue;
                    }
                    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
                    if path_has_shared_track(
                        &layout.original_edges,
                        first_index,
                        &first_candidate,
                        &replacements,
                    ) {
                        continue;
                    }
                    if path_has_node_hit(
                        &layout.original_edges[second_index],
                        &second_candidate,
                        &real_node_rects,
                        work_budget,
                    )? {
                        continue;
                    }
                    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
                    if path_has_shared_track(
                        &layout.original_edges,
                        second_index,
                        &second_candidate,
                        &replacements,
                    ) {
                        continue;
                    }
                    let candidate_crossings = checked_strict_crossing_count(
                        &layout.original_edges,
                        &replacements,
                        work_budget,
                    )?;
                    let candidate_bends =
                        checked_total_bends(&layout.original_edges, &replacements, work_budget)?;
                    if candidate_crossings >= current_crossings
                        || candidate_crossings > best_crossings
                        || (candidate_crossings == best_crossings && candidate_bends >= best_bends)
                    {
                        continue;
                    }
                    best_replacements = Some(replacements);
                    best_crossings = candidate_crossings;
                    best_bends = candidate_bends;
                }
            }
        }
        let Some(replacements) = best_replacements else {
            return Ok(());
        };
        apply_replacements(&mut layout.original_edges, replacements);
    }
    Ok(())
}
