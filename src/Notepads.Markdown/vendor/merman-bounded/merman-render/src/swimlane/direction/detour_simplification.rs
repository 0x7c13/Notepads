use super::super::working::WorkingLayout;
use super::LayoutWorkBudget;
use super::geometry::{
    EPSILON, NodeBoundsInfo, OrthogonalSegment, RectEntry, RectSide, build_orthogonal_port_path,
    build_same_side_track_path, collect_real_node_bounds, count_orthogonal_bends,
    orthogonal_segments_cross, orthogonal_segments_for_points, port_for_rect_side,
    same_axis_segment_overlap_length, segment_hits_any_rect,
};
use crate::Result;
use crate::model::LayoutPoint;
use std::collections::HashMap;

const MINIMUM_SHARED_LENGTH: f64 = 8.0;
const ANCHOR_OFFSET: f64 = 20.0;
const BEND_THRESHOLD: usize = 4;

const SIDES: [RectSide; 4] = [
    RectSide::Top,
    RectSide::Bottom,
    RectSide::Left,
    RectSide::Right,
];

#[derive(Debug, Clone, Copy)]
struct OutsideTracks {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

impl OutsideTracks {
    fn for_side(self, side: RectSide) -> f64 {
        match side {
            RectSide::Top => self.top,
            RectSide::Bottom => self.bottom,
            RectSide::Left => self.left,
            RectSide::Right => self.right,
        }
    }
}

#[derive(Debug, Clone)]
struct FaceClaim {
    side: RectSide,
    edge_id: String,
}

fn outside_tracks(rects: &[RectEntry]) -> OutsideTracks {
    let mut top = f64::INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for entry in rects {
        top = top.min(entry.rect.top);
        bottom = bottom.max(entry.rect.bottom);
        left = left.min(entry.rect.left);
        right = right.max(entry.rect.right);
    }
    OutsideTracks {
        top: top - ANCHOR_OFFSET,
        bottom: bottom + ANCHOR_OFFSET,
        left: left - ANCHOR_OFFSET,
        right: right + ANCHOR_OFFSET,
    }
}

fn build_path_candidates(
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    tracks: OutsideTracks,
) -> Vec<Vec<LayoutPoint>> {
    let mut paths = Vec::new();
    if let Some(path) = build_orthogonal_port_path(
        source,
        source_side,
        destination,
        destination_side,
        ANCHOR_OFFSET,
        EPSILON,
    ) {
        paths.push(path);
    }
    if source_side == destination_side {
        paths.push(build_same_side_track_path(
            source,
            source_side,
            destination,
            tracks.for_side(source_side),
        ));
    }
    paths
}

fn path_hits_node(
    points: &[LayoutPoint],
    node_rects: &[RectEntry],
    excluded_ids: &[&str],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    work_budget.charge(
        points
            .len()
            .saturating_sub(1)
            .saturating_mul(node_rects.len()),
    )?;
    Ok(points.windows(2).any(|segment| {
        segment_hits_any_rect(&segment[0], &segment[1], node_rects, excluded_ids, 1.0)
    }))
}

fn segments_conflict(first: &OrthogonalSegment, second: &OrthogonalSegment) -> bool {
    orthogonal_segments_cross(&first.a, &first.b, &second.a, &second.b, EPSILON, EPSILON)
        || same_axis_segment_overlap_length(first, second, EPSILON) >= MINIMUM_SHARED_LENGTH
}

fn path_conflict_count(
    layout: &WorkingLayout,
    edge_index: usize,
    points: &[LayoutPoint],
    include_incident_edges: bool,
    work_budget: &mut LayoutWorkBudget,
) -> Result<usize> {
    let current = &layout.original_edges[edge_index];
    let path_segments = orthogonal_segments_for_points(points, EPSILON);
    let mut conflicts = 0;

    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
    for (other_index, other) in layout.original_edges.iter().enumerate() {
        if other_index == edge_index {
            continue;
        }
        if !include_incident_edges
            && (other.from == current.from
                || other.from == current.to
                || other.to == current.from
                || other.to == current.to)
        {
            continue;
        }
        if other.points.len() < 2 {
            continue;
        }
        let other_segments = orthogonal_segments_for_points(&other.points, EPSILON);
        for path_segment in &path_segments {
            for other_segment in &other_segments {
                if segments_conflict(path_segment, other_segment) {
                    conflicts += 1;
                }
            }
        }
    }
    Ok(conflicts)
}

fn nearest_side_of_rect(point: &LayoutPoint, info: &NodeBoundsInfo) -> RectSide {
    let distances = [
        (RectSide::Top, (point.y - info.rect.top).abs()),
        (RectSide::Bottom, (point.y - info.rect.bottom).abs()),
        (RectSide::Left, (point.x - info.rect.left).abs()),
        (RectSide::Right, (point.x - info.rect.right).abs()),
    ];
    let mut best = distances[0];
    for candidate in distances.into_iter().skip(1) {
        if candidate.1 < best.1 {
            best = candidate;
        }
    }
    best.0
}

fn add_face_claim(
    claims: &mut HashMap<String, Vec<FaceClaim>>,
    node_id: &str,
    side: RectSide,
    edge_id: &str,
) {
    claims
        .entry(node_id.to_string())
        .or_default()
        .push(FaceClaim {
            side,
            edge_id: edge_id.to_string(),
        });
}

fn face_is_claimed(
    claims: &HashMap<String, Vec<FaceClaim>>,
    node_id: &str,
    side: RectSide,
    ignored_edge_id: &str,
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let node_claims = claims.get(node_id);
    work_budget.charge(node_claims.map_or(0, Vec::len))?;
    Ok(node_claims.is_some_and(|node_claims| {
        node_claims
            .iter()
            .any(|claim| claim.edge_id != ignored_edge_id && claim.side == side)
    }))
}

pub(super) fn simplify_detoured_edges(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (node_info_by_id, real_node_rects) = collect_real_node_bounds(layout);
    work_budget.charge(real_node_rects.len())?;
    let tracks = outside_tracks(&real_node_rects);
    let mut face_claims: HashMap<String, Vec<FaceClaim>> = HashMap::new();

    work_budget.charge(layout.original_edges.len())?;
    for edge in &layout.original_edges {
        let Some(first) = edge.points.first() else {
            continue;
        };
        if let Some(info) = node_info_by_id.get(&edge.from) {
            add_face_claim(
                &mut face_claims,
                &edge.from,
                nearest_side_of_rect(first, info),
                &edge.id,
            );
        }
        if let (Some(last), Some(info)) = (edge.points.last(), node_info_by_id.get(&edge.to)) {
            add_face_claim(
                &mut face_claims,
                &edge.to,
                nearest_side_of_rect(last, info),
                &edge.id,
            );
        }
    }

    work_budget.charge(layout.original_edges.len())?;
    for edge_index in 0..layout.original_edges.len() {
        let edge = layout.original_edges[edge_index].clone();
        if edge.points.len() < 2 {
            continue;
        }
        let current_bends = count_orthogonal_bends(&edge.points, EPSILON);
        if current_bends < BEND_THRESHOLD {
            continue;
        }
        let (Some(source_info), Some(destination_info)) = (
            node_info_by_id.get(&edge.from).cloned(),
            node_info_by_id.get(&edge.to).cloned(),
        ) else {
            continue;
        };

        let current_crossing_conflicts =
            path_conflict_count(layout, edge_index, &edge.points, true, work_budget)?;
        let current_non_incident_conflicts =
            path_conflict_count(layout, edge_index, &edge.points, false, work_budget)?;
        let mut best_path = None;
        let mut best_crossing_conflicts = current_crossing_conflicts;
        let mut best_bends = current_bends;

        for source_side in SIDES {
            if face_is_claimed(&face_claims, &edge.from, source_side, &edge.id, work_budget)? {
                continue;
            }
            let source_port = port_for_rect_side(&source_info, source_side);
            for destination_side in SIDES {
                if face_is_claimed(
                    &face_claims,
                    &edge.to,
                    destination_side,
                    &edge.id,
                    work_budget,
                )? {
                    continue;
                }
                let destination_port = port_for_rect_side(&destination_info, destination_side);
                for path in build_path_candidates(
                    &source_port,
                    source_side,
                    &destination_port,
                    destination_side,
                    tracks,
                ) {
                    work_budget.charge(1)?;
                    let excluded_ids = [edge.from.as_str(), edge.to.as_str()];
                    if path_hits_node(&path, &real_node_rects, &excluded_ids, work_budget)? {
                        continue;
                    }

                    let path_bends = count_orthogonal_bends(&path, EPSILON);
                    if current_crossing_conflicts > 0 {
                        let path_crossing_conflicts =
                            path_conflict_count(layout, edge_index, &path, true, work_budget)?;
                        if path_crossing_conflicts > best_crossing_conflicts
                            || (path_crossing_conflicts == best_crossing_conflicts
                                && path_bends >= best_bends)
                        {
                            continue;
                        }
                        best_crossing_conflicts = path_crossing_conflicts;
                        best_bends = path_bends;
                        best_path = Some(path);
                        continue;
                    }

                    if path_conflict_count(layout, edge_index, &path, false, work_budget)?
                        > current_non_incident_conflicts
                    {
                        continue;
                    }
                    if path_bends < best_bends {
                        best_bends = path_bends;
                        best_path = Some(path);
                    }
                }
            }
        }

        let Some(best_path) = best_path else {
            continue;
        };
        work_budget.charge(face_claims.get(&edge.from).map_or(0, Vec::len))?;
        work_budget.charge(face_claims.get(&edge.to).map_or(0, Vec::len))?;
        layout.original_edges[edge_index].points = best_path.clone();
        if let Some(claims) = face_claims.get_mut(&edge.from) {
            claims.retain(|claim| claim.edge_id != edge.id);
        }
        if let Some(claims) = face_claims.get_mut(&edge.to) {
            claims.retain(|claim| claim.edge_id != edge.id);
        }
        add_face_claim(
            &mut face_claims,
            &edge.from,
            nearest_side_of_rect(&best_path[0], &source_info),
            &edge.id,
        );
        add_face_claim(
            &mut face_claims,
            &edge.to,
            nearest_side_of_rect(
                best_path.last().expect("a routed path is non-empty"),
                &destination_info,
            ),
            &edge.id,
        );
    }
    Ok(())
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

    fn node(id: &str, x: f64, y: f64) -> WorkingNode {
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
            width: 40.0,
            height: 40.0,
            label_width: 40.0,
            label_height: 40.0,
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
                .collect::<IndexMap<_, _>>(),
            graph_edges: Vec::new(),
            original_edges: edges,
            top_lane_order: Vec::new(),
        }
    }

    fn detoured_edge() -> WorkingEdge {
        edge(
            "A_B",
            "A",
            "B",
            vec![
                point(20.0, 0.0),
                point(40.0, 0.0),
                point(40.0, -40.0),
                point(80.0, -40.0),
                point(80.0, 100.0),
                point(100.0, 100.0),
            ],
        )
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual.x - expected.0).abs() < EPSILON);
            assert!((actual.y - expected.1).abs() < EPSILON);
        }
    }

    #[test]
    fn rewrites_a_four_bend_detour_to_the_best_safe_port_path() {
        let mut layout = layout(
            vec![node("A", 0.0, 0.0), node("B", 100.0, 100.0)],
            vec![detoured_edge()],
        );

        simplify_detoured_edges(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests()).unwrap();

        assert_points(
            &layout.original_edges[0].points,
            &[(0.0, 20.0), (0.0, 100.0), (80.0, 100.0)],
        );
    }

    #[test]
    fn rejects_every_candidate_when_destination_faces_are_claimed() {
        let original = detoured_edge();
        let mut layout = layout(
            vec![node("A", 0.0, 0.0), node("B", 100.0, 100.0)],
            vec![
                original.clone(),
                edge(
                    "claim_top",
                    "missing_top",
                    "B",
                    vec![point(100.0, 60.0), point(100.0, 80.0)],
                ),
                edge(
                    "claim_bottom",
                    "missing_bottom",
                    "B",
                    vec![point(100.0, 140.0), point(100.0, 120.0)],
                ),
                edge(
                    "claim_left",
                    "missing_left",
                    "B",
                    vec![point(60.0, 100.0), point(80.0, 100.0)],
                ),
                edge(
                    "claim_right",
                    "missing_right",
                    "B",
                    vec![point(140.0, 100.0), point(120.0, 100.0)],
                ),
            ],
        );

        simplify_detoured_edges(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests()).unwrap();

        assert_points(
            &layout.original_edges[0].points,
            &original
                .points
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
    }
}
