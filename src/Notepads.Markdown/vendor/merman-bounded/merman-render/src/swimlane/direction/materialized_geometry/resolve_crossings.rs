use super::common::*;
use crate::Result;
use crate::swimlane::direction::{LayoutWorkBudget, unordered_pair_count};
use indexmap::{IndexMap, IndexSet};

const ANCHOR: f64 = 20.0;
const EXTRA_CHANNEL_COUNT: usize = 2;
const MAX_ITERATIONS: usize = 4;
const MAX_PAIR_CANDIDATES_PER_EDGE: usize = 48;
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
struct CrossingPair {
    first: usize,
    second: usize,
    count: usize,
}

#[derive(Debug, Clone)]
struct CrossingSnapshot {
    count: usize,
    pairs: Vec<CrossingPair>,
    edge_set: IndexSet<usize>,
    edges: Vec<usize>,
}

#[derive(Debug, Clone)]
struct PairCandidate {
    path: Vec<LayoutPoint>,
    segments: Vec<OrthogonalSegment>,
    shared_track_conflicts: IndexSet<usize>,
    total_bends: usize,
    length: f64,
}

#[derive(Debug, Clone)]
struct PairOption {
    edge_index: usize,
    candidates: Vec<PairCandidate>,
}

#[derive(Debug, Clone)]
struct PairReplacementScore {
    replacements: ReplacementMap,
    crossings: usize,
    bends: usize,
    length: f64,
}

struct PairCandidateSearch<'a> {
    edges: &'a [WorkingEdge],
    current: &'a CrossingSnapshot,
    base_segments: &'a IndexMap<usize, Vec<OrthogonalSegment>>,
    crossing_count_by_edge: &'a IndexMap<usize, usize>,
    node_info_by_id: &'a IndexMap<String, NodeBoundsInfo>,
    real_node_rects: &'a [RectEntry],
    outside: OutsideTracks,
}

struct PairScoringContext<'a> {
    edges: &'a [WorkingEdge],
    current: &'a CrossingSnapshot,
    current_bends: usize,
    current_length: f64,
    base_bends: &'a IndexMap<usize, usize>,
    base_lengths: &'a IndexMap<usize, f64>,
    base_segments: &'a IndexMap<usize, Vec<OrthogonalSegment>>,
}

fn outside_tracks(node_info_by_id: &IndexMap<String, NodeBoundsInfo>) -> Option<OutsideTracks> {
    (!node_info_by_id.is_empty()).then(|| OutsideTracks {
        top: node_info_by_id
            .values()
            .map(|node| node.rect.top)
            .fold(f64::INFINITY, f64::min)
            - ANCHOR,
        bottom: node_info_by_id
            .values()
            .map(|node| node.rect.bottom)
            .fold(f64::NEG_INFINITY, f64::max)
            + ANCHOR,
        left: node_info_by_id
            .values()
            .map(|node| node.rect.left)
            .fold(f64::INFINITY, f64::min)
            - ANCHOR,
        right: node_info_by_id
            .values()
            .map(|node| node.rect.right)
            .fold(f64::NEG_INFINITY, f64::max)
            + ANCHOR,
    })
}

fn outward_tracks_for_side(side: RectSide, outside: OutsideTracks) -> Vec<f64> {
    let outward = if matches!(side, RectSide::Left | RectSide::Top) {
        -1.0
    } else {
        1.0
    };
    (0..=EXTRA_CHANNEL_COUNT)
        .map(|channel| outside.for_side(side) + outward * ANCHOR * channel as f64)
        .collect()
}

fn crossing_snapshot(edges: &[WorkingEdge], replacements: &ReplacementMap) -> CrossingSnapshot {
    let mut count = 0;
    let mut pairs = Vec::new();
    let mut edge_set = IndexSet::new();
    let mut edge_order = Vec::new();
    for first in 0..edges.len() {
        let first_points = points_for(edges, first, replacements);
        for second in first + 1..edges.len() {
            let pair_count = crossing_count_between_paths(
                &first_points,
                &points_for(edges, second, replacements),
            );
            if pair_count == 0 {
                continue;
            }
            count += pair_count;
            pairs.push(CrossingPair {
                first,
                second,
                count: pair_count,
            });
            if edge_set.insert(first) {
                edge_order.push(first);
            }
            if edge_set.insert(second) {
                edge_order.push(second);
            }
        }
    }
    edge_order.sort_unstable();
    CrossingSnapshot {
        count,
        pairs,
        edge_set,
        edges: edge_order,
    }
}

fn crossing_count_with_replacements(
    edges: &[WorkingEdge],
    current: &CrossingSnapshot,
    replacements: &ReplacementMap,
    work_budget: &mut LayoutWorkBudget,
) -> Result<usize> {
    if replacements.is_empty() {
        return Ok(current.count);
    }
    work_budget.charge(current.pairs.len())?;
    let current_affected: usize = current
        .pairs
        .iter()
        .filter(|pair| {
            replacements.contains_key(&pair.first) || replacements.contains_key(&pair.second)
        })
        .map(|pair| pair.count)
        .sum();
    work_budget.charge(unordered_pair_count(edges.len()))?;
    let mut replacement_affected = 0;
    for first in 0..edges.len() {
        let first_changed = replacements.contains_key(&first);
        let first_points = points_for(edges, first, replacements);
        for second in first + 1..edges.len() {
            if !first_changed && !replacements.contains_key(&second) {
                continue;
            }
            replacement_affected += crossing_count_between_paths(
                &first_points,
                &points_for(edges, second, replacements),
            );
        }
    }
    Ok(current.count - current_affected + replacement_affected)
}

fn crossing_components(snapshot: &CrossingSnapshot) -> Vec<Vec<usize>> {
    let mut neighbors: IndexMap<usize, IndexSet<usize>> = IndexMap::new();
    for pair in &snapshot.pairs {
        neighbors.entry(pair.first).or_default().insert(pair.second);
        neighbors.entry(pair.second).or_default().insert(pair.first);
    }
    let mut components = Vec::new();
    let mut seen = IndexSet::new();
    for &edge_index in &snapshot.edges {
        if seen.contains(&edge_index) {
            continue;
        }
        let mut queue = vec![edge_index];
        let mut component = Vec::new();
        seen.insert(edge_index);
        while let Some(current) = queue.pop() {
            component.push(current);
            if let Some(next_edges) = neighbors.get(&current) {
                for &next in next_edges {
                    if seen.insert(next) {
                        queue.push(next);
                    }
                }
            }
        }
        component.sort_unstable();
        if component.len() > 1 {
            components.push(component);
        }
    }
    components
}

fn pair_search_groups(
    edges: &[WorkingEdge],
    snapshot: &CrossingSnapshot,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<Vec<usize>>> {
    work_budget.charge(
        snapshot
            .pairs
            .len()
            .saturating_mul(3)
            .saturating_add(snapshot.edges.len()),
    )?;
    let mut groups = Vec::new();
    for component in crossing_components(snapshot) {
        work_budget.charge(component.len().saturating_mul(3))?;
        let component_set: IndexSet<_> = component.iter().copied().collect();
        let endpoint_ids: IndexSet<_> = component
            .iter()
            .flat_map(|&edge_index| [edges[edge_index].from.clone(), edges[edge_index].to.clone()])
            .collect();
        let mut group = component;
        work_budget.charge(edges.len())?;
        for (edge_index, edge) in edges.iter().enumerate() {
            if !component_set.contains(&edge_index)
                && (endpoint_ids.contains(&edge.from) || endpoint_ids.contains(&edge.to))
            {
                group.push(edge_index);
            }
        }
        group.sort_unstable();
        groups.push(group);
    }
    Ok(groups)
}

fn current_crossings_by_edge(snapshot: &CrossingSnapshot) -> IndexMap<usize, usize> {
    let mut result = IndexMap::new();
    for pair in &snapshot.pairs {
        *result.entry(pair.first).or_default() += pair.count;
        *result.entry(pair.second).or_default() += pair.count;
    }
    result
}

fn path_has_segment_conflict(
    edges: &[WorkingEdge],
    edge_index: usize,
    path: &[LayoutPoint],
    replacements: &ReplacementMap,
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let candidate_segments = segments_for(path);
    work_budget.charge(edges.len().saturating_sub(1))?;
    Ok((0..edges.len()).any(|other_index| {
        other_index != edge_index
            && shared_track_conflicts(
                &candidate_segments,
                &segments_for(&points_for(edges, other_index, replacements)),
            )
    }))
}

fn path_hits_node(
    edge: &WorkingEdge,
    path: &[LayoutPoint],
    real_node_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let excluded = endpoint_id_slices(edge);
    let segments = segments_for(path);
    work_budget.charge(segments.len().saturating_mul(real_node_rects.len()))?;
    Ok(segments.iter().any(|segment| {
        segment_hits_any_rect(&segment.a, &segment.b, real_node_rects, &excluded, -2.0)
    }))
}

fn side_is_horizontal(side: RectSide) -> bool {
    matches!(side, RectSide::Left | RectSide::Right)
}

fn local_track_for_same_side(
    source: &LayoutPoint,
    side: RectSide,
    destination: &LayoutPoint,
) -> f64 {
    match side {
        RectSide::Left => source.x.min(destination.x) - ANCHOR,
        RectSide::Right => source.x.max(destination.x) + ANCHOR,
        RectSide::Top => source.y.min(destination.y) - ANCHOR,
        RectSide::Bottom => source.y.max(destination.y) + ANCHOR,
    }
}

fn add_same_side_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    outside: OutsideTracks,
) {
    let outward = if matches!(source_side, RectSide::Left | RectSide::Top) {
        -1.0
    } else {
        1.0
    };
    let seeds = [
        local_track_for_same_side(source, source_side, destination),
        outside.for_side(source_side),
    ];
    for seed in seeds {
        for channel in 0..=EXTRA_CHANNEL_COUNT {
            push_orthogonal_candidate(
                candidates,
                build_same_side_track_path(
                    source,
                    source_side,
                    destination,
                    seed + outward * ANCHOR * channel as f64,
                ),
            );
        }
    }
}

fn add_horizontal_to_vertical_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    outside: OutsideTracks,
) {
    for x_track in outward_tracks_for_side(source_side, outside) {
        for y_track in outward_tracks_for_side(destination_side, outside) {
            push_orthogonal_candidate(
                candidates,
                vec![
                    source.clone(),
                    LayoutPoint {
                        x: x_track,
                        y: source.y,
                    },
                    LayoutPoint {
                        x: x_track,
                        y: y_track,
                    },
                    LayoutPoint {
                        x: destination.x,
                        y: y_track,
                    },
                    destination.clone(),
                ],
            );
        }
    }
}

fn add_vertical_to_horizontal_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    outside: OutsideTracks,
) {
    for y_track in outward_tracks_for_side(source_side, outside) {
        for x_track in outward_tracks_for_side(destination_side, outside) {
            push_orthogonal_candidate(
                candidates,
                vec![
                    source.clone(),
                    LayoutPoint {
                        x: source.x,
                        y: y_track,
                    },
                    LayoutPoint {
                        x: x_track,
                        y: y_track,
                    },
                    LayoutPoint {
                        x: x_track,
                        y: destination.y,
                    },
                    destination.clone(),
                ],
            );
        }
    }
}

fn add_horizontal_pair_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    outside: OutsideTracks,
) {
    let y_tracks = outward_tracks_for_side(RectSide::Top, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Bottom, outside));
    let y_tracks: Vec<_> = y_tracks.collect();
    for source_track in outward_tracks_for_side(source_side, outside) {
        for destination_track in outward_tracks_for_side(destination_side, outside) {
            for &y_track in &y_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        source.clone(),
                        LayoutPoint {
                            x: source_track,
                            y: source.y,
                        },
                        LayoutPoint {
                            x: source_track,
                            y: y_track,
                        },
                        LayoutPoint {
                            x: destination_track,
                            y: y_track,
                        },
                        LayoutPoint {
                            x: destination_track,
                            y: destination.y,
                        },
                        destination.clone(),
                    ],
                );
            }
        }
    }
}

fn add_vertical_pair_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    outside: OutsideTracks,
) {
    let x_tracks = outward_tracks_for_side(RectSide::Left, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Right, outside));
    let x_tracks: Vec<_> = x_tracks.collect();
    for source_track in outward_tracks_for_side(source_side, outside) {
        for destination_track in outward_tracks_for_side(destination_side, outside) {
            for &x_track in &x_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        source.clone(),
                        LayoutPoint {
                            x: source.x,
                            y: source_track,
                        },
                        LayoutPoint {
                            x: x_track,
                            y: source_track,
                        },
                        LayoutPoint {
                            x: x_track,
                            y: destination_track,
                        },
                        LayoutPoint {
                            x: destination.x,
                            y: destination_track,
                        },
                        destination.clone(),
                    ],
                );
            }
        }
    }
}

fn build_candidates_for_sides(
    source: &LayoutPoint,
    source_side: RectSide,
    destination: &LayoutPoint,
    destination_side: RectSide,
    outside: OutsideTracks,
) -> Vec<Vec<LayoutPoint>> {
    let mut candidates = Vec::new();
    if let Some(base) = build_orthogonal_port_path(
        source,
        source_side,
        destination,
        destination_side,
        ANCHOR,
        EPSILON,
    ) {
        push_orthogonal_candidate(&mut candidates, base);
    }
    if source_side == destination_side {
        add_same_side_candidates(&mut candidates, source, source_side, destination, outside);
    }
    let source_horizontal = side_is_horizontal(source_side);
    let destination_horizontal = side_is_horizontal(destination_side);
    if source_horizontal && !destination_horizontal {
        add_horizontal_to_vertical_candidates(
            &mut candidates,
            source,
            source_side,
            destination,
            destination_side,
            outside,
        );
    } else if !source_horizontal && destination_horizontal {
        add_vertical_to_horizontal_candidates(
            &mut candidates,
            source,
            source_side,
            destination,
            destination_side,
            outside,
        );
    } else if source_horizontal {
        add_horizontal_pair_candidates(
            &mut candidates,
            source,
            source_side,
            destination,
            destination_side,
            outside,
        );
    } else {
        add_vertical_pair_candidates(
            &mut candidates,
            source,
            source_side,
            destination,
            destination_side,
            outside,
        );
    }
    dedupe_candidate_paths(candidates)
}

fn add_vertical_departure_outer_track_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    first: &LayoutPoint,
    departure: &LayoutPoint,
    destination_node: &NodeBoundsInfo,
    outside: OutsideTracks,
) {
    let external_x_tracks: Vec<_> = outward_tracks_for_side(RectSide::Left, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Right, outside))
        .collect();
    let external_y_tracks: Vec<_> = outward_tracks_for_side(RectSide::Top, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Bottom, outside))
        .collect();
    for side in SIDES {
        let destination = port_for_rect_side(destination_node, side);
        let target_y_tracks = if matches!(side, RectSide::Top | RectSide::Bottom) {
            outward_tracks_for_side(side, outside)
        } else {
            external_y_tracks.clone()
        };
        for &track in &external_x_tracks {
            push_orthogonal_candidate(
                candidates,
                vec![
                    first.clone(),
                    departure.clone(),
                    LayoutPoint {
                        x: track,
                        y: departure.y,
                    },
                    LayoutPoint {
                        x: track,
                        y: destination.y,
                    },
                    destination.clone(),
                ],
            );
            for &target_track in &target_y_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first.clone(),
                        departure.clone(),
                        LayoutPoint {
                            x: track,
                            y: departure.y,
                        },
                        LayoutPoint {
                            x: track,
                            y: target_track,
                        },
                        LayoutPoint {
                            x: destination.x,
                            y: target_track,
                        },
                        destination.clone(),
                    ],
                );
            }
        }
    }
}

fn add_horizontal_departure_outer_track_candidates(
    candidates: &mut Vec<Vec<LayoutPoint>>,
    first: &LayoutPoint,
    departure: &LayoutPoint,
    destination_node: &NodeBoundsInfo,
    outside: OutsideTracks,
) {
    let external_x_tracks: Vec<_> = outward_tracks_for_side(RectSide::Left, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Right, outside))
        .collect();
    let external_y_tracks: Vec<_> = outward_tracks_for_side(RectSide::Top, outside)
        .into_iter()
        .chain(outward_tracks_for_side(RectSide::Bottom, outside))
        .collect();
    for side in SIDES {
        let destination = port_for_rect_side(destination_node, side);
        let target_x_tracks = if matches!(side, RectSide::Left | RectSide::Right) {
            outward_tracks_for_side(side, outside)
        } else {
            external_x_tracks.clone()
        };
        for &track in &external_y_tracks {
            push_orthogonal_candidate(
                candidates,
                vec![
                    first.clone(),
                    departure.clone(),
                    LayoutPoint {
                        x: departure.x,
                        y: track,
                    },
                    LayoutPoint {
                        x: destination.x,
                        y: track,
                    },
                    destination.clone(),
                ],
            );
            for &target_track in &target_x_tracks {
                push_orthogonal_candidate(
                    candidates,
                    vec![
                        first.clone(),
                        departure.clone(),
                        LayoutPoint {
                            x: departure.x,
                            y: track,
                        },
                        LayoutPoint {
                            x: target_track,
                            y: track,
                        },
                        LayoutPoint {
                            x: target_track,
                            y: destination.y,
                        },
                        destination.clone(),
                    ],
                );
            }
        }
    }
}

fn terminal_preserving_outer_track_candidates(
    edge: &WorkingEdge,
    node_info_by_id: &IndexMap<String, NodeBoundsInfo>,
    outside: OutsideTracks,
) -> Vec<Vec<LayoutPoint>> {
    if edge.from.is_empty() {
        return Vec::new();
    }
    let Some(destination_node) = node_info_by_id.get(&edge.to) else {
        return Vec::new();
    };
    let points = dedupe_consecutive_points(&edge.points, EPSILON);
    if points.len() < 4 {
        return Vec::new();
    }
    let first = &points[0];
    let departure = &points[1];
    let mut candidates = Vec::new();
    if is_vertical_segment(first, departure, EPSILON) {
        add_vertical_departure_outer_track_candidates(
            &mut candidates,
            first,
            departure,
            destination_node,
            outside,
        );
    } else if is_horizontal_segment(first, departure, EPSILON) {
        add_horizontal_departure_outer_track_candidates(
            &mut candidates,
            first,
            departure,
            destination_node,
            outside,
        );
    }
    candidates
}

fn candidate_paths_for(
    edge: &WorkingEdge,
    node_info_by_id: &IndexMap<String, NodeBoundsInfo>,
    outside: OutsideTracks,
) -> Vec<Vec<LayoutPoint>> {
    let (Some(source_node), Some(destination_node)) = (
        node_info_by_id.get(&edge.from),
        node_info_by_id.get(&edge.to),
    ) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for source_side in SIDES {
        let source_port = port_for_rect_side(source_node, source_side);
        for destination_side in SIDES {
            candidates.extend(build_candidates_for_sides(
                &source_port,
                source_side,
                &port_for_rect_side(destination_node, destination_side),
                destination_side,
                outside,
            ));
        }
    }
    candidates.extend(terminal_preserving_outer_track_candidates(
        edge,
        node_info_by_id,
        outside,
    ));
    candidates
}

fn current_segments_by_edge(edges: &[WorkingEdge]) -> IndexMap<usize, Vec<OrthogonalSegment>> {
    (0..edges.len())
        .map(|edge_index| {
            (
                edge_index,
                segments_for(&points_for(edges, edge_index, &ReplacementMap::new())),
            )
        })
        .collect()
}

fn shared_track_conflicts_for(
    edges: &[WorkingEdge],
    edge_index: usize,
    candidate_segments: &[OrthogonalSegment],
    base_segments: &IndexMap<usize, Vec<OrthogonalSegment>>,
    work_budget: &mut LayoutWorkBudget,
) -> Result<IndexSet<usize>> {
    work_budget.charge(edges.len().saturating_sub(1))?;
    let mut conflicts = IndexSet::new();
    for (other_index, other_edge) in edges.iter().enumerate() {
        if other_index == edge_index {
            continue;
        }
        let fallback;
        let other_segments = if let Some(segments) = base_segments.get(&other_index) {
            segments
        } else {
            fallback = segments_for(&dedupe_consecutive_points(&other_edge.points, EPSILON));
            &fallback
        };
        if shared_track_conflicts(candidate_segments, other_segments) {
            conflicts.insert(other_index);
        }
    }
    Ok(conflicts)
}

fn pair_candidates_for(
    search: &PairCandidateSearch<'_>,
    edge_index: usize,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Vec<PairCandidate>> {
    struct ScoredCandidate {
        path: Vec<LayoutPoint>,
        segments: Vec<OrthogonalSegment>,
        crossings: usize,
        bends: usize,
        total_bends: usize,
        length: f64,
    }

    let edges = search.edges;
    let mut seen = IndexSet::new();
    let mut candidates = Vec::new();
    for raw_candidate in
        candidate_paths_for(&edges[edge_index], search.node_info_by_id, search.outside)
    {
        work_budget.charge(1)?;
        let candidate = simplify_polyline(&dedupe_consecutive_points(&raw_candidate, EPSILON));
        if path_hits_node(
            &edges[edge_index],
            &candidate,
            search.real_node_rects,
            work_budget,
        )? || candidate.len() < 2
            || !seen.insert(candidate_key(&candidate))
        {
            continue;
        }
        let candidate_segments = segments_for(&candidate);
        work_budget.charge(edges.len().saturating_sub(1))?;
        let replacement_affected: usize = (0..edges.len())
            .filter(|&other_index| other_index != edge_index)
            .map(|other_index| {
                crossing_count_between_segments(
                    &candidate_segments,
                    &search.base_segments[&other_index],
                )
            })
            .sum();
        let crossings = search.current.count
            - search
                .crossing_count_by_edge
                .get(&edge_index)
                .copied()
                .unwrap_or(0)
            + replacement_affected;
        if crossings <= search.current.count {
            candidates.push(ScoredCandidate {
                path: candidate.clone(),
                segments: candidate_segments,
                crossings,
                bends: count_orthogonal_bends(&candidate, EPSILON),
                total_bends: count_orthogonal_bends(&candidate, EPSILON),
                length: manhattan_path_length(&candidate),
            });
        }
    }
    candidates.sort_by(|first, second| {
        first
            .crossings
            .cmp(&second.crossings)
            .then_with(|| first.bends.cmp(&second.bends))
            .then_with(|| first.length.total_cmp(&second.length))
    });
    let mut retained = Vec::new();
    for candidate in candidates.into_iter().take(MAX_PAIR_CANDIDATES_PER_EDGE) {
        retained.push(PairCandidate {
            shared_track_conflicts: shared_track_conflicts_for(
                edges,
                edge_index,
                &candidate.segments,
                search.base_segments,
                work_budget,
            )?,
            path: candidate.path,
            segments: candidate.segments,
            total_bends: candidate.total_bends,
            length: candidate.length,
        });
    }
    Ok(retained)
}

fn pair_crossing_count(
    context: &PairScoringContext<'_>,
    first_edge: usize,
    first_candidate: &PairCandidate,
    second_edge: usize,
    second_candidate: &PairCandidate,
    work_budget: &mut LayoutWorkBudget,
) -> Result<usize> {
    work_budget.charge(context.current.pairs.len())?;
    let current_affected: usize = context
        .current
        .pairs
        .iter()
        .filter(|pair| {
            pair.first == first_edge
                || pair.second == first_edge
                || pair.first == second_edge
                || pair.second == second_edge
        })
        .map(|pair| pair.count)
        .sum();
    work_budget
        .charge(1usize.saturating_add(context.edges.len().saturating_sub(2).saturating_mul(2)))?;
    let mut replacement_affected =
        crossing_count_between_segments(&first_candidate.segments, &second_candidate.segments);
    for other_index in 0..context.edges.len() {
        if other_index == first_edge || other_index == second_edge {
            continue;
        }
        replacement_affected += crossing_count_between_segments(
            &first_candidate.segments,
            &context.base_segments[&other_index],
        ) + crossing_count_between_segments(
            &second_candidate.segments,
            &context.base_segments[&other_index],
        );
    }
    Ok(context.current.count - current_affected + replacement_affected)
}

fn conflicts_only_with(candidate: &PairCandidate, edge_index: usize) -> bool {
    candidate
        .shared_track_conflicts
        .iter()
        .all(|&conflict| conflict == edge_index)
}

fn candidates_share_track(first: &PairCandidate, second: &PairCandidate) -> bool {
    shared_track_conflicts(&first.segments, &second.segments)
}

fn pair_candidates_are_compatible(
    first: &PairOption,
    first_candidate: &PairCandidate,
    second: &PairOption,
    second_candidate: &PairCandidate,
) -> bool {
    conflicts_only_with(first_candidate, second.edge_index)
        && conflicts_only_with(second_candidate, first.edge_index)
        && !candidates_share_track(first_candidate, second_candidate)
}

fn score_pair_replacement(
    context: &PairScoringContext<'_>,
    first: &PairOption,
    first_candidate: &PairCandidate,
    second: &PairOption,
    second_candidate: &PairCandidate,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<PairReplacementScore>> {
    let crossings = pair_crossing_count(
        context,
        first.edge_index,
        first_candidate,
        second.edge_index,
        second_candidate,
        work_budget,
    )?;
    if crossings >= context.current.count {
        return Ok(None);
    }
    Ok(Some(PairReplacementScore {
        replacements: ReplacementMap::from_iter([
            (first.edge_index, first_candidate.path.clone()),
            (second.edge_index, second_candidate.path.clone()),
        ]),
        crossings,
        bends: context.current_bends
            - context.base_bends[&first.edge_index]
            - context.base_bends[&second.edge_index]
            + first_candidate.total_bends
            + second_candidate.total_bends,
        length: context.current_length
            - context.base_lengths[&first.edge_index]
            - context.base_lengths[&second.edge_index]
            + first_candidate.length
            + second_candidate.length,
    }))
}

fn pair_score_is_better(candidate: &PairReplacementScore, best: &PairReplacementScore) -> bool {
    candidate.crossings < best.crossings
        || (candidate.crossings == best.crossings
            && (candidate.bends < best.bends
                || (candidate.bends == best.bends && candidate.length < best.length)))
}

fn best_score_for_option_pair(
    context: &PairScoringContext<'_>,
    first: &PairOption,
    second: &PairOption,
    mut best: PairReplacementScore,
    work_budget: &mut LayoutWorkBudget,
) -> Result<PairReplacementScore> {
    for first_candidate in &first.candidates {
        for second_candidate in &second.candidates {
            work_budget.charge(1)?;
            if !pair_candidates_are_compatible(first, first_candidate, second, second_candidate) {
                continue;
            }
            let Some(score) = score_pair_replacement(
                context,
                first,
                first_candidate,
                second,
                second_candidate,
                work_budget,
            )?
            else {
                continue;
            };
            if pair_score_is_better(&score, &best) {
                best = score;
            }
        }
    }
    Ok(best)
}

fn best_paired_replacement(
    edges: &[WorkingEdge],
    current: &CrossingSnapshot,
    node_info_by_id: &IndexMap<String, NodeBoundsInfo>,
    real_node_rects: &[RectEntry],
    outside: OutsideTracks,
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<ReplacementMap>> {
    let empty = ReplacementMap::new();
    let current_bends = checked_total_bends(edges, &empty, work_budget)?;
    work_budget.charge(edges.len())?;
    let current_length: f64 = (0..edges.len())
        .map(|edge_index| manhattan_path_length(&points_for(edges, edge_index, &empty)))
        .sum();
    work_budget.charge(edges.len())?;
    let base_segments = current_segments_by_edge(edges);
    work_budget.charge(current.pairs.len())?;
    let crossing_count_by_edge = current_crossings_by_edge(current);
    work_budget.charge(edges.len())?;
    let base_bends: IndexMap<_, _> = (0..edges.len())
        .map(|edge_index| {
            (
                edge_index,
                count_orthogonal_bends(&points_for(edges, edge_index, &empty), EPSILON),
            )
        })
        .collect();
    work_budget.charge(edges.len())?;
    let base_lengths: IndexMap<_, _> = (0..edges.len())
        .map(|edge_index| {
            (
                edge_index,
                manhattan_path_length(&points_for(edges, edge_index, &empty)),
            )
        })
        .collect();
    let candidate_search = PairCandidateSearch {
        edges,
        current,
        base_segments: &base_segments,
        crossing_count_by_edge: &crossing_count_by_edge,
        node_info_by_id,
        real_node_rects,
        outside,
    };
    let groups = pair_search_groups(edges, current, work_budget)?;
    let mut options_by_edge = IndexMap::new();
    for group in &groups {
        for &edge_index in group {
            if options_by_edge.contains_key(&edge_index) {
                continue;
            }
            let candidates = pair_candidates_for(&candidate_search, edge_index, work_budget)?;
            if !candidates.is_empty() {
                options_by_edge.insert(
                    edge_index,
                    PairOption {
                        edge_index,
                        candidates,
                    },
                );
            }
        }
    }

    let scoring = PairScoringContext {
        edges,
        current,
        current_bends,
        current_length,
        base_bends: &base_bends,
        base_lengths: &base_lengths,
        base_segments: &base_segments,
    };

    let mut best = PairReplacementScore {
        replacements: ReplacementMap::new(),
        crossings: current.count,
        bends: current_bends,
        length: current_length,
    };
    for group in groups {
        let crossing_edge_set: IndexSet<_> = group
            .iter()
            .copied()
            .filter(|edge_index| current.edge_set.contains(edge_index))
            .collect();
        let options: Vec<_> = group
            .iter()
            .filter_map(|edge_index| options_by_edge.get(edge_index))
            .collect();
        for first_index in 0..options.len() {
            for second_index in first_index + 1..options.len() {
                work_budget.charge(1)?;
                let first = options[first_index];
                let second = options[second_index];
                if !crossing_edge_set.contains(&first.edge_index)
                    && !crossing_edge_set.contains(&second.edge_index)
                {
                    continue;
                }
                best = best_score_for_option_pair(&scoring, first, second, best, work_budget)?;
            }
        }
    }
    Ok((!best.replacements.is_empty()).then_some(best.replacements))
}

pub(in crate::swimlane::direction) fn resolve_rendered_orthogonal_crossings(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (node_info_by_id, real_node_rects) = collect_real_node_bounds(layout);
    work_budget.charge(node_info_by_id.len().saturating_mul(4))?;
    let Some(outside) = outside_tracks(&node_info_by_id) else {
        return Ok(());
    };

    for _ in 0..MAX_ITERATIONS {
        work_budget.charge(unordered_pair_count(layout.original_edges.len()))?;
        let current = crossing_snapshot(&layout.original_edges, &ReplacementMap::new());
        if current.count == 0 {
            return Ok(());
        }
        let mut best_edge = None;
        let mut best_path = None;
        let mut best_crossings = current.count;
        let mut best_bends = usize::MAX;

        for &edge_index in &current.edges {
            let current_edge_bends = count_orthogonal_bends(
                &dedupe_consecutive_points(&layout.original_edges[edge_index].points, EPSILON),
                EPSILON,
            );
            for candidate in candidate_paths_for(
                &layout.original_edges[edge_index],
                &node_info_by_id,
                outside,
            ) {
                work_budget.charge(1)?;
                let candidate_hits_node = path_hits_node(
                    &layout.original_edges[edge_index],
                    &candidate,
                    &real_node_rects,
                    work_budget,
                )?;
                let candidate_has_segment_conflict = if candidate_hits_node {
                    false
                } else {
                    path_has_segment_conflict(
                        &layout.original_edges,
                        edge_index,
                        &candidate,
                        &ReplacementMap::new(),
                        work_budget,
                    )?
                };
                let replacements = ReplacementMap::from_iter([(edge_index, candidate.clone())]);
                let candidate_crossings = crossing_count_with_replacements(
                    &layout.original_edges,
                    &current,
                    &replacements,
                    work_budget,
                )?;
                let candidate_bends = count_orthogonal_bends(&candidate, EPSILON);
                if candidate_hits_node || candidate_has_segment_conflict {
                    continue;
                }
                let improves_current_edge = candidate_crossings < current.count
                    || (candidate_crossings == current.count
                        && candidate_bends < current_edge_bends);
                if !improves_current_edge
                    || candidate_crossings > best_crossings
                    || (candidate_crossings == best_crossings && candidate_bends >= best_bends)
                {
                    continue;
                }
                best_edge = Some(edge_index);
                best_path = Some(candidate);
                best_crossings = candidate_crossings;
                best_bends = candidate_bends;
            }
        }
        if let (Some(edge_index), Some(points)) = (best_edge, best_path) {
            layout.original_edges[edge_index].points = points;
            continue;
        }

        let Some(replacements) = best_paired_replacement(
            &layout.original_edges,
            &current,
            &node_info_by_id,
            &real_node_rects,
            outside,
            work_budget,
        )?
        else {
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

    fn edge(id: &str, y: f64) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: format!("{id}-from"),
            to: format!("{id}-to"),
            reference_id: id.to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points: vec![LayoutPoint { x: 0.0, y }, LayoutPoint { x: 20.0, y }],
        }
    }

    fn pair_candidate(y: f64) -> PairCandidate {
        let path = vec![LayoutPoint { x: 0.0, y }, LayoutPoint { x: 20.0, y }];
        PairCandidate {
            segments: segments_for(&path),
            path,
            shared_track_conflicts: IndexSet::new(),
            total_bends: 0,
            length: 20.0,
        }
    }

    #[test]
    fn paired_candidate_unit_can_pass_before_internal_edge_pair_scoring_is_rejected() {
        let edges = vec![
            edge("first", 0.0),
            edge("second", 10.0),
            edge("third", 20.0),
            edge("fourth", 30.0),
        ];
        let current = crossing_snapshot(&edges, &ReplacementMap::new());
        let base_segments = current_segments_by_edge(&edges);
        let base_bends = (0..edges.len()).map(|index| (index, 0)).collect();
        let base_lengths = (0..edges.len()).map(|index| (index, 20.0)).collect();
        let context = PairScoringContext {
            edges: &edges,
            current: &current,
            current_bends: 0,
            current_length: 80.0,
            base_bends: &base_bends,
            base_lengths: &base_lengths,
            base_segments: &base_segments,
        };
        let first = PairOption {
            edge_index: 0,
            candidates: vec![pair_candidate(0.0)],
        };
        let second = PairOption {
            edge_index: 1,
            candidates: vec![pair_candidate(10.0)],
        };
        let best = PairReplacementScore {
            replacements: ReplacementMap::new(),
            crossings: 1,
            bends: 0,
            length: 80.0,
        };
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error =
            best_score_for_option_pair(&context, &first, &second, best, &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 6);
        assert_eq!(error.max, 1);
        assert_eq!(budget.used(), 1);
    }
}
