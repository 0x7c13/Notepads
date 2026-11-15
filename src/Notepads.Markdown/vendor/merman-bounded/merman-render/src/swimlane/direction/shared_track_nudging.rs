use super::super::working::{WorkingEdge, WorkingLayout};
use super::LayoutWorkBudget;
use super::geometry::{
    EPSILON, OrthogonalSegment, RectBounds, RectEntry, collect_node_rect_entries,
    dedupe_consecutive_points, orthogonal_segments_for_points, orthogonal_segments_strictly_cross,
    overlap_length, rect_of_node_bounds, segment_hits_any_rect,
};
use crate::Result;
use crate::model::LayoutPoint;

const MINIMUM_SHARED_LENGTH: f64 = 8.0;
const TRACK_SHIFT: f64 = 7.0;
const MINIMUM_TRACK_GAP: f64 = TRACK_SHIFT;
const SOURCE_DETOUR_STUB: f64 = 20.0;
const OBSTACLE_BUFFER: f64 = 2.0;
const MAXIMUM_ITERATIONS: usize = 12;
const SHIFTS: [f64; 6] = [
    -TRACK_SHIFT,
    TRACK_SHIFT,
    -2.0 * TRACK_SHIFT,
    2.0 * TRACK_SHIFT,
    -3.0 * TRACK_SHIFT,
    3.0 * TRACK_SHIFT,
];

#[derive(Debug, Clone)]
struct Segment {
    edge_index: usize,
    geometry: OrthogonalSegment,
    interior: bool,
}

#[derive(Debug, Clone)]
struct SourceDetourContext {
    source_center: LayoutPoint,
    target_center: LayoutPoint,
    source_rect: RectBounds,
    tail: Vec<LayoutPoint>,
}

fn segments_for(edge_index: usize, points: &[LayoutPoint]) -> Vec<Segment> {
    orthogonal_segments_for_points(points, EPSILON)
        .into_iter()
        .map(|geometry| Segment {
            edge_index,
            interior: geometry.index >= 1 && geometry.index + 3 <= points.len(),
            geometry,
        })
        .collect()
}

fn all_segments(layout: &WorkingLayout) -> Vec<Segment> {
    let mut result = Vec::new();
    for (edge_index, edge) in layout.original_edges.iter().enumerate() {
        if edge.points.len() < 2 {
            continue;
        }
        let points = dedupe_consecutive_points(&edge.points, EPSILON);
        result.extend(segments_for(edge_index, &points));
    }
    result
}

fn has_crowded_parallel_track(first: &Segment, second: &Segment) -> bool {
    let first = &first.geometry;
    let second = &second.geometry;
    if first.horizontal && second.horizontal {
        return overlap_length(first.a.x, first.b.x, second.a.x, second.b.x)
            >= MINIMUM_SHARED_LENGTH
            && (first.a.y - second.a.y).abs() < MINIMUM_TRACK_GAP;
    }
    if first.vertical && second.vertical {
        return overlap_length(first.a.y, first.b.y, second.a.y, second.b.y)
            >= MINIMUM_SHARED_LENGTH
            && (first.a.x - second.a.x).abs() < MINIMUM_TRACK_GAP;
    }
    false
}

fn endpoint_ids(edge: &WorkingEdge) -> Vec<&str> {
    [&edge.from, &edge.to]
        .into_iter()
        .filter_map(|id| (!id.is_empty()).then_some(id.as_str()))
        .collect()
}

fn candidate_is_safe(
    layout: &WorkingLayout,
    edge_index: usize,
    candidate: &[LayoutPoint],
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    let candidate_segments = segments_for(edge_index, candidate);
    if candidate_segments.len() != candidate.len().saturating_sub(1) {
        return Ok(false);
    }

    let edge = &layout.original_edges[edge_index];
    let endpoint_ids = endpoint_ids(edge);
    let own_label_ids = edge
        .label_node_id
        .as_deref()
        .into_iter()
        .collect::<Vec<_>>();
    for segment in &candidate_segments {
        work_budget.charge(real_node_rects.len())?;
        if segment_hits_any_rect(
            &segment.geometry.a,
            &segment.geometry.b,
            real_node_rects,
            &endpoint_ids,
            -OBSTACLE_BUFFER,
        ) {
            return Ok(false);
        }
        work_budget.charge(label_rects.len())?;
        if segment_hits_any_rect(
            &segment.geometry.a,
            &segment.geometry.b,
            label_rects,
            &own_label_ids,
            -OBSTACLE_BUFFER,
        ) {
            return Ok(false);
        }
    }

    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
    for (other_index, other) in layout.original_edges.iter().enumerate() {
        if other_index == edge_index || other.points.len() < 2 {
            continue;
        }
        let other_points = dedupe_consecutive_points(&other.points, EPSILON);
        let other_segments = segments_for(other_index, &other_points);
        for candidate_segment in &candidate_segments {
            for other_segment in &other_segments {
                if has_crowded_parallel_track(candidate_segment, other_segment)
                    || orthogonal_segments_strictly_cross(
                        &candidate_segment.geometry.a,
                        &candidate_segment.geometry.b,
                        &other_segment.geometry.a,
                        &other_segment.geometry.b,
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

fn shifted_candidate(
    layout: &WorkingLayout,
    segment: &Segment,
    shift: f64,
) -> Option<Vec<LayoutPoint>> {
    let edge = &layout.original_edges[segment.edge_index];
    let mut candidate = dedupe_consecutive_points(&edge.points, EPSILON);
    let index = segment.geometry.index;
    if candidate.len() < 4 || index >= candidate.len() - 1 {
        return None;
    }

    if segment.geometry.horizontal {
        candidate[index].y += shift;
        candidate[index + 1].y += shift;
    } else if segment.geometry.vertical {
        candidate[index].x += shift;
        candidate[index + 1].x += shift;
    } else {
        return None;
    }
    (segments_for(segment.edge_index, &candidate).len() == candidate.len() - 1).then_some(candidate)
}

fn source_detour_context_for(
    layout: &WorkingLayout,
    segment: &Segment,
) -> Option<SourceDetourContext> {
    let edge = &layout.original_edges[segment.edge_index];
    let points = dedupe_consecutive_points(&edge.points, EPSILON);
    if points.len() != 4 || segment.geometry.index != 1 {
        return None;
    }
    if edge.from.is_empty() || edge.to.is_empty() {
        return None;
    }

    let source = layout.nodes.get(&edge.from)?;
    let target = layout.nodes.get(&edge.to)?;
    let source_rect = rect_of_node_bounds(source)?;
    rect_of_node_bounds(target)?;
    let tail = points[segment.geometry.index + 2..].to_vec();
    if tail.is_empty() {
        return None;
    }

    Some(SourceDetourContext {
        source_center: LayoutPoint {
            x: source.x,
            y: source.y,
        },
        target_center: LayoutPoint {
            x: target.x,
            y: target.y,
        },
        source_rect,
        tail,
    })
}

fn vertical_source_detour(
    segment: &Segment,
    shift: f64,
    context: &SourceDetourContext,
) -> Option<Vec<LayoutPoint>> {
    let target_below = context.target_center.y >= context.source_center.y;
    let source_port_y = if target_below {
        context.source_rect.bottom
    } else {
        context.source_rect.top
    };
    let stub_y = source_port_y
        + if target_below {
            SOURCE_DETOUR_STUB
        } else {
            -SOURCE_DETOUR_STUB
        };
    if (target_below && segment.geometry.b.y <= stub_y + EPSILON)
        || (!target_below && segment.geometry.b.y >= stub_y - EPSILON)
    {
        return None;
    }

    let rail_x = segment.geometry.a.x + shift;
    let mut points = vec![
        LayoutPoint {
            x: context.source_center.x,
            y: source_port_y,
        },
        LayoutPoint {
            x: context.source_center.x,
            y: stub_y,
        },
        LayoutPoint {
            x: rail_x,
            y: stub_y,
        },
        LayoutPoint {
            x: rail_x,
            y: segment.geometry.b.y,
        },
    ];
    points.extend(context.tail.iter().cloned());
    Some(dedupe_consecutive_points(&points, EPSILON))
}

fn horizontal_source_detour(
    segment: &Segment,
    shift: f64,
    context: &SourceDetourContext,
) -> Option<Vec<LayoutPoint>> {
    let target_right = context.target_center.x >= context.source_center.x;
    let source_port_x = if target_right {
        context.source_rect.right
    } else {
        context.source_rect.left
    };
    let stub_x = source_port_x
        + if target_right {
            SOURCE_DETOUR_STUB
        } else {
            -SOURCE_DETOUR_STUB
        };
    if (target_right && segment.geometry.b.x <= stub_x + EPSILON)
        || (!target_right && segment.geometry.b.x >= stub_x - EPSILON)
    {
        return None;
    }

    let rail_y = segment.geometry.a.y + shift;
    let mut points = vec![
        LayoutPoint {
            x: source_port_x,
            y: context.source_center.y,
        },
        LayoutPoint {
            x: stub_x,
            y: context.source_center.y,
        },
        LayoutPoint {
            x: stub_x,
            y: rail_y,
        },
        LayoutPoint {
            x: segment.geometry.b.x,
            y: rail_y,
        },
    ];
    points.extend(context.tail.iter().cloned());
    Some(dedupe_consecutive_points(&points, EPSILON))
}

fn source_detour_candidate(
    layout: &WorkingLayout,
    segment: &Segment,
    shift: f64,
) -> Option<Vec<LayoutPoint>> {
    let context = source_detour_context_for(layout, segment)?;
    if segment.geometry.vertical {
        return vertical_source_detour(segment, shift, &context);
    }
    if segment.geometry.horizontal {
        return horizontal_source_detour(segment, shift, &context);
    }
    None
}

fn first_safe_rewrite(
    layout: &WorkingLayout,
    segments: &[Segment],
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<(usize, Vec<LayoutPoint>)>> {
    for (first_index, first) in segments.iter().enumerate() {
        for second in &segments[first_index + 1..] {
            work_budget.charge(1)?;
            if first.edge_index == second.edge_index || !has_crowded_parallel_track(first, second) {
                continue;
            }

            for segment in [first, second]
                .into_iter()
                .filter(|segment| segment.interior)
            {
                for shift in SHIFTS {
                    work_budget.charge(layout.original_edges[segment.edge_index].points.len())?;
                    if let Some(candidate) = shifted_candidate(layout, segment, shift) {
                        work_budget.charge(1)?;
                        if candidate_is_safe(
                            layout,
                            segment.edge_index,
                            &candidate,
                            real_node_rects,
                            label_rects,
                            work_budget,
                        )? {
                            return Ok(Some((segment.edge_index, candidate)));
                        }
                    }
                    work_budget.charge(layout.original_edges[segment.edge_index].points.len())?;
                    if let Some(candidate) = source_detour_candidate(layout, segment, shift) {
                        work_budget.charge(1)?;
                        if candidate_is_safe(
                            layout,
                            segment.edge_index,
                            &candidate,
                            real_node_rects,
                            label_rects,
                            work_budget,
                        )? {
                            return Ok(Some((segment.edge_index, candidate)));
                        }
                    }
                }
            }
        }
    }
    Ok(None)
}

pub(super) fn nudge_shared_interior_subpaths(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, label_rects) = collect_node_rect_entries(layout);
    for _ in 0..MAXIMUM_ITERATIONS {
        work_budget.charge(layout.original_edges.len())?;
        let edge_point_work = layout.original_edges.iter().fold(0usize, |total, edge| {
            total.saturating_add(edge.points.len())
        });
        work_budget.charge(edge_point_work)?;
        let segments = all_segments(layout);
        let Some((edge_index, points)) = first_safe_rewrite(
            layout,
            &segments,
            &real_node_rects,
            &label_rects,
            work_budget,
        )?
        else {
            return Ok(());
        };
        layout.original_edges[edge_index].points = points;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::working::{WorkingEdge, WorkingNode, WorkingNodeKind};
    use super::*;
    use crate::model::{LayoutPoint, SwimlaneDirection};
    use indexmap::IndexMap;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
    }

    fn edge(
        id: &str,
        from: &str,
        to: &str,
        points: Vec<LayoutPoint>,
        label_node_id: Option<&str>,
    ) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            reference_id: id.to_string(),
            label_node_id: label_node_id.map(str::to_string),
            reversed_for_layout: false,
            points,
        }
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

    fn vertical_segment_x(points: &[LayoutPoint], minimum_length: f64) -> Option<f64> {
        points.windows(2).find_map(|pair| {
            ((pair[0].x - pair[1].x).abs() < 1e-3
                && (pair[0].y - pair[1].y).abs() >= minimum_length)
                .then_some(pair[0].x)
        })
    }

    fn vertical_segment_x_overlapping_y(
        points: &[LayoutPoint],
        first_y: f64,
        second_y: f64,
    ) -> Option<f64> {
        points.windows(2).find_map(|pair| {
            if (pair[0].x - pair[1].x).abs() >= 1e-3 {
                return None;
            }
            let overlap =
                pair[0].y.max(pair[1].y).min(second_y) - pair[0].y.min(pair[1].y).max(first_y);
            (overlap.max(0.0) > 0.0).then_some(pair[0].x)
        })
    }

    #[test]
    fn separates_near_parallel_interior_rails_with_long_projected_overlap() {
        let mut layout = layout(
            Vec::new(),
            vec![
                edge(
                    "detoured",
                    "",
                    "",
                    vec![
                        point(0.0, 0.0),
                        point(50.0, 0.0),
                        point(50.0, 100.0),
                        point(80.0, 100.0),
                    ],
                    None,
                ),
                edge(
                    "nearby",
                    "",
                    "",
                    vec![point(52.0, 0.0), point(52.0, 100.0)],
                    None,
                ),
            ],
        );

        nudge_shared_interior_subpaths(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        let detoured_x = vertical_segment_x(&layout.original_edges[0].points, 80.0).unwrap();
        assert!((detoured_x - 52.0).abs() >= 7.0);
    }

    #[test]
    fn separates_the_close_fixture_twelve_exit_rails_with_a_source_detour() {
        let mut layout = layout(
            vec![
                node(
                    "I",
                    WorkingNodeKind::Content,
                    -131.5906219482422,
                    973.5,
                    35.89374923706055,
                    45.0,
                ),
                node(
                    "B",
                    WorkingNodeKind::Content,
                    -131.5906219482422,
                    146.5,
                    121.3125,
                    91.0,
                ),
                node(
                    "exit",
                    WorkingNodeKind::Content,
                    267.21875,
                    1083.5643920898438,
                    53.34375,
                    51.0,
                ),
            ],
            vec![
                edge(
                    "L_I_exit_0",
                    "I",
                    "exit",
                    vec![
                        point(-113.64374732971191, 955.0),
                        point(220.83124923706055, 955.0),
                        point(220.83124923706055, 1083.5643920898438),
                        point(240.546875, 1083.5643920898438),
                    ],
                    None,
                ),
                edge(
                    "L_H_I_0",
                    "",
                    "",
                    vec![
                        point(243.83124923706055, 973.5),
                        point(-113.64374732971191, 973.5),
                    ],
                    None,
                ),
                edge(
                    "L_B_exit_0",
                    "B",
                    "exit",
                    vec![
                        point(-131.5906219482422, 192.0),
                        point(-131.5906219482422, 928.0),
                        point(222.77187538146973, 928.0),
                        point(222.77187538146973, 1038.0643920898438),
                        point(258.328125, 1038.0643920898438),
                        point(258.328125, 1058.0643920898438),
                    ],
                    None,
                ),
            ],
        );

        nudge_shared_interior_subpaths(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        let first_x = vertical_segment_x_overlapping_y(
            &layout.original_edges[0].points,
            1016.0,
            1038.0643920898438,
        )
        .unwrap();
        let second_x = vertical_segment_x_overlapping_y(
            &layout.original_edges[2].points,
            1016.0,
            1038.0643920898438,
        )
        .unwrap();
        assert!((first_x - second_x).abs() >= 7.0);
        let start = &layout.original_edges[0].points[0];
        assert!((start.x - -131.5906219482422).abs() < 1e-9);
        assert!((start.y - 996.0).abs() < 1e-9);
    }

    #[test]
    fn nudges_a_labelled_edge_while_excluding_its_own_label_obstacle() {
        let mut layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 0.0, 0.0, 40.0, 40.0),
                node("B", WorkingNodeKind::Content, 100.0, 100.0, 40.0, 40.0),
                node(
                    "edge-label-A-B",
                    WorkingNodeKind::EdgeLabel,
                    50.0,
                    50.0,
                    70.0,
                    10.0,
                ),
            ],
            vec![
                edge(
                    "A_B",
                    "A",
                    "B",
                    vec![
                        point(20.0, 0.0),
                        point(50.0, 0.0),
                        point(50.0, 100.0),
                        point(80.0, 100.0),
                    ],
                    Some("edge-label-A-B"),
                ),
                edge(
                    "foreign",
                    "",
                    "",
                    vec![point(50.0, 0.0), point(50.0, 100.0)],
                    None,
                ),
            ],
        );

        nudge_shared_interior_subpaths(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        let labelled_x = vertical_segment_x(&layout.original_edges[0].points, 80.0).unwrap();
        assert!((labelled_x - 50.0).abs() >= 7.0);
    }

    #[test]
    fn tries_shifts_in_source_order_when_a_node_blocks_the_first_candidate() {
        let mut layout = layout(
            vec![node(
                "blocker",
                WorkingNodeKind::Content,
                43.0,
                50.0,
                10.0,
                80.0,
            )],
            vec![
                edge(
                    "detoured",
                    "",
                    "",
                    vec![
                        point(0.0, 0.0),
                        point(50.0, 0.0),
                        point(50.0, 100.0),
                        point(80.0, 100.0),
                    ],
                    None,
                ),
                edge(
                    "nearby",
                    "",
                    "",
                    vec![point(52.0, 0.0), point(52.0, 100.0)],
                    None,
                ),
            ],
        );

        nudge_shared_interior_subpaths(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_eq!(
            vertical_segment_x(&layout.original_edges[0].points, 80.0),
            Some(36.0)
        );
    }

    #[test]
    fn leaves_crowded_terminal_only_tracks_unchanged() {
        let mut layout = layout(
            Vec::new(),
            vec![
                edge(
                    "first",
                    "",
                    "",
                    vec![point(50.0, 0.0), point(50.0, 100.0)],
                    None,
                ),
                edge(
                    "second",
                    "",
                    "",
                    vec![point(52.0, 0.0), point(52.0, 100.0)],
                    None,
                ),
            ],
        );
        let before = layout.original_edges[0].points.clone();

        nudge_shared_interior_subpaths(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_eq!(layout.original_edges[0].points.len(), before.len());
        for (actual, expected) in layout.original_edges[0].points.iter().zip(before) {
            assert!((actual.x - expected.x).abs() < 1e-9);
            assert!((actual.y - expected.y).abs() < 1e-9);
        }
    }

    #[test]
    fn candidate_safety_rejects_before_the_full_edge_scan() {
        let edges = (0..100)
            .map(|index| {
                edge(
                    &format!("edge-{index}"),
                    "",
                    "",
                    vec![
                        point(1_000.0 + index as f64 * 20.0, 0.0),
                        point(1_000.0 + index as f64 * 20.0, 10.0),
                    ],
                    None,
                )
            })
            .collect::<Vec<_>>();
        let layout = layout(
            vec![node(
                "far-node",
                WorkingNodeKind::Content,
                -1_000.0,
                -1_000.0,
                10.0,
                10.0,
            )],
            edges,
        );
        let before = layout
            .original_edges
            .iter()
            .map(|edge| {
                edge.points
                    .iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let (real_node_rects, label_rects) = collect_node_rect_entries(&layout);
        assert_eq!(real_node_rects.len(), 1);
        assert!(label_rects.is_empty());
        let policy = crate::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 1)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error = candidate_is_safe(
            &layout,
            0,
            &[point(0.0, 0.0), point(10.0, 0.0)],
            &real_node_rects,
            &label_rects,
            &mut budget,
        )
        .unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        // The segment-vs-node scan consumes the exact boundary before the 99-edge scan.
        assert_eq!(budget.used(), 1);
        assert_eq!(error.actual, 100);
        assert_eq!(error.max, 1);
        assert_eq!(
            layout
                .original_edges
                .iter()
                .map(|edge| {
                    edge.points
                        .iter()
                        .map(|point| (point.x, point.y))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            before
        );
    }
}
