use super::common::*;
use crate::Result;
use crate::swimlane::direction::LayoutWorkBudget;

const BUFFER: f64 = 2.0;
const MAX_ITERATIONS: usize = 8;

fn candidate_is_safe(
    edges: &[WorkingEdge],
    edge_index: usize,
    candidate: &[LayoutPoint],
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
        if other_index == edge_index || other.points.len() < 2 {
            continue;
        }
        let other_points = dedupe_consecutive_points(&other.points, EPSILON);
        let other_segments = segments_for(&other_points);
        for candidate_segment in &candidate_segments {
            for other_segment in &other_segments {
                if same_axis_segment_overlap_length(candidate_segment, other_segment, 0.5)
                    >= MINIMUM_SHARED_LENGTH
                    || orthogonal_segments_strictly_cross(
                        &candidate_segment.a,
                        &candidate_segment.b,
                        &other_segment.a,
                        &other_segment.b,
                        EPSILON,
                    )
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn without_dogleg(points: &[LayoutPoint], index: usize) -> Option<Vec<LayoutPoint>> {
    if index + 4 >= points.len() {
        return None;
    }
    let p0 = &points[index];
    let p1 = &points[index + 1];
    let p2 = &points[index + 2];
    let p3 = &points[index + 3];
    let p4 = &points[index + 4];

    let terminal_vertical_dogleg = is_horizontal_segment(p0, p1, EPSILON)
        && is_vertical_segment(p1, p2, EPSILON)
        && is_horizontal_segment(p2, p3, EPSILON)
        && is_vertical_segment(p3, p4, EPSILON)
        && same_x(p0, p3, EPSILON)
        && same_x(p0, p4, EPSILON)
        && same_x(p1, p2, EPSILON)
        && (p1.x - p0.x) * (p3.x - p2.x) < 0.0;
    let terminal_horizontal_dogleg = is_vertical_segment(p0, p1, EPSILON)
        && is_horizontal_segment(p1, p2, EPSILON)
        && is_vertical_segment(p2, p3, EPSILON)
        && is_horizontal_segment(p3, p4, EPSILON)
        && same_y(p0, p3, EPSILON)
        && same_y(p0, p4, EPSILON)
        && same_y(p1, p2, EPSILON)
        && (p1.y - p0.y) * (p3.y - p2.y) < 0.0;
    if terminal_vertical_dogleg || terminal_horizontal_dogleg {
        let mut candidate = points[..=index].to_vec();
        candidate.push(p4.clone());
        candidate.extend_from_slice(&points[index + 5..]);
        return Some(dedupe_consecutive_points(&candidate, EPSILON));
    }

    if index + 5 >= points.len() {
        return None;
    }
    let p5 = &points[index + 5];
    let vertical_dogleg = is_vertical_segment(p0, p1, EPSILON)
        && is_horizontal_segment(p1, p2, EPSILON)
        && is_vertical_segment(p2, p3, EPSILON)
        && is_horizontal_segment(p3, p4, EPSILON)
        && is_vertical_segment(p4, p5, EPSILON)
        && same_x(p0, p4, EPSILON)
        && same_x(p0, p5, EPSILON)
        && same_x(p2, p3, EPSILON)
        && (p2.x - p1.x) * (p4.x - p3.x) < 0.0;
    let horizontal_dogleg = is_horizontal_segment(p0, p1, EPSILON)
        && is_vertical_segment(p1, p2, EPSILON)
        && is_horizontal_segment(p2, p3, EPSILON)
        && is_vertical_segment(p3, p4, EPSILON)
        && is_horizontal_segment(p4, p5, EPSILON)
        && same_y(p0, p4, EPSILON)
        && same_y(p0, p5, EPSILON)
        && same_y(p2, p3, EPSILON)
        && (p2.y - p1.y) * (p4.y - p3.y) < 0.0;
    if !vertical_dogleg && !horizontal_dogleg {
        return None;
    }

    let mut candidate = points[..=index].to_vec();
    candidate.push(p5.clone());
    candidate.extend_from_slice(&points[index + 6..]);
    Some(dedupe_consecutive_points(&candidate, EPSILON))
}

pub(in crate::swimlane::direction) fn collapse_redundant_rectangular_doglegs(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, label_rects) = collect_node_rect_entries(layout);
    for _ in 0..MAX_ITERATIONS {
        let mut replacement = None;
        work_budget.charge(layout.original_edges.len())?;
        for (edge_index, edge) in layout.original_edges.iter().enumerate() {
            let points = dedupe_consecutive_points(&edge.points, EPSILON);
            if points.len() < 5 {
                continue;
            }
            for index in 0..=points.len() - 5 {
                let Some(candidate) = without_dogleg(&points, index) else {
                    continue;
                };
                work_budget.charge(1)?;
                if candidate_is_safe(
                    &layout.original_edges,
                    edge_index,
                    &candidate,
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
