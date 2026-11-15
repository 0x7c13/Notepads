use super::super::working::WorkingLayout;
use super::LayoutWorkBudget;
use super::geometry::{
    ThreeSegmentRouteKind, classify_three_segment_route, collect_real_node_bounds,
    dedupe_consecutive_points, get_node_pair_geometry, same_point, segment_conflicts_with_any_edge,
    segment_hits_any_rect,
};
use crate::Result;
use crate::model::LayoutPoint;
use crate::swimlane::config::EPSILON;

const MINIMUM_PORT_SPACING: f64 = 8.0;
const PORT_SHIFT: f64 = MINIMUM_PORT_SPACING;
const TRY_DELTAS: [f64; 5] = [
    0.0,
    PORT_SHIFT,
    -PORT_SHIFT,
    2.0 * PORT_SHIFT,
    -2.0 * PORT_SHIFT,
];

pub(super) fn port_swap_to_l_shape(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (node_info_by_id, real_node_rects) = collect_real_node_bounds(layout);

    work_budget.charge(layout.original_edges.len())?;
    for edge_index in 0..layout.original_edges.len() {
        let edge = layout.original_edges[edge_index].clone();
        if edge.points.len() < 4 {
            continue;
        }

        let deduped = dedupe_consecutive_points(&edge.points, EPSILON);
        let Some(route) = classify_three_segment_route(&deduped, EPSILON) else {
            continue;
        };
        let Some(node_pair) = get_node_pair_geometry(&edge, &node_info_by_id, EPSILON) else {
            continue;
        };
        if node_pair.collinear_x || node_pair.collinear_y {
            continue;
        }

        let is_hvh = route.kind == ThreeSegmentRouteKind::Hvh;
        let mut replacement = None;
        for delta in TRY_DELTAS {
            let (first, corner, destination) = if is_hvh {
                let destination_below = node_pair.dst_info.cy > node_pair.src_info.cy;
                let source_y = if destination_below {
                    node_pair.src_info.rect.bottom
                } else {
                    node_pair.src_info.rect.top
                };
                let source_x = node_pair.src_info.cx + delta;
                if source_x <= node_pair.src_info.rect.left + EPSILON
                    || source_x >= node_pair.src_info.rect.right - EPSILON
                {
                    continue;
                }
                (
                    LayoutPoint {
                        x: source_x,
                        y: source_y,
                    },
                    LayoutPoint {
                        x: source_x,
                        y: route.p3.y,
                    },
                    route.p3.clone(),
                )
            } else {
                let destination_east = node_pair.dst_info.cx > node_pair.src_info.cx;
                let source_x = if destination_east {
                    node_pair.src_info.rect.right
                } else {
                    node_pair.src_info.rect.left
                };
                let source_y = node_pair.src_info.cy + delta;
                if source_y <= node_pair.src_info.rect.top + EPSILON
                    || source_y >= node_pair.src_info.rect.bottom - EPSILON
                {
                    continue;
                }
                (
                    LayoutPoint {
                        x: source_x,
                        y: source_y,
                    },
                    LayoutPoint {
                        x: route.p3.x,
                        y: source_y,
                    },
                    route.p3.clone(),
                )
            };

            let first_segment_degenerate = same_point(&first, &corner, EPSILON);
            let second_segment_degenerate = same_point(&corner, &destination, EPSILON);
            if first_segment_degenerate && second_segment_degenerate {
                continue;
            }

            work_budget.charge(1)?;
            if !first_segment_degenerate {
                work_budget.charge(real_node_rects.len())?;
                if segment_hits_any_rect(
                    &first,
                    &corner,
                    &real_node_rects,
                    &[node_pair.src_id.as_str()],
                    1.0,
                ) {
                    continue;
                }
            }
            if !second_segment_degenerate {
                work_budget.charge(real_node_rects.len())?;
                if segment_hits_any_rect(
                    &corner,
                    &destination,
                    &real_node_rects,
                    &[node_pair.dst_id.as_str()],
                    1.0,
                ) {
                    continue;
                }
            }

            let first_segment_conflicts = if first_segment_degenerate {
                false
            } else {
                work_budget.charge(layout.original_edges.len())?;
                segment_conflicts_with_any_edge(
                    &first,
                    &corner,
                    &layout.original_edges,
                    Some(edge.id.as_str()),
                    EPSILON,
                    true,
                )
            };
            let second_segment_conflicts = if second_segment_degenerate {
                false
            } else {
                work_budget.charge(layout.original_edges.len())?;
                segment_conflicts_with_any_edge(
                    &corner,
                    &destination,
                    &layout.original_edges,
                    Some(edge.id.as_str()),
                    EPSILON,
                    true,
                )
            };
            if first_segment_conflicts || second_segment_conflicts {
                continue;
            }

            replacement = Some(if first_segment_degenerate {
                vec![corner, destination]
            } else if second_segment_degenerate {
                vec![first, corner]
            } else {
                vec![first, corner, destination]
            });
            break;
        }

        if let Some(points) = replacement {
            layout.original_edges[edge_index].points = points;
        }
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

    fn node(id: &str, x: f64, y: f64, width: f64, height: f64) -> WorkingNode {
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

    fn edge(points: Vec<LayoutPoint>) -> WorkingEdge {
        WorkingEdge {
            id: "A_B".to_string(),
            from: "A".to_string(),
            to: "B".to_string(),
            reference_id: "A_B".to_string(),
            label_node_id: None,
            reversed_for_layout: false,
            points,
        }
    }

    fn layout(nodes: Vec<WorkingNode>, edge: WorkingEdge) -> WorkingLayout {
        WorkingLayout {
            direction: SwimlaneDirection::Tb,
            nodes: nodes
                .into_iter()
                .map(|node| (node.id.clone(), node))
                .collect::<IndexMap<_, _>>(),
            graph_edges: Vec::new(),
            original_edges: vec![edge],
            top_lane_order: Vec::new(),
        }
    }

    fn detour() -> Vec<LayoutPoint> {
        vec![
            point(20.0, 0.0),
            point(60.0, 0.0),
            point(60.0, 80.0),
            point(100.0, 80.0),
        ]
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual.x - expected.0).abs() < EPSILON);
            assert!((actual.y - expected.1).abs() < EPSILON);
        }
    }

    #[test]
    fn rewrites_a_non_collinear_hvh_detour_to_an_l_shape() {
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 40.0, 40.0),
                node("B", 100.0, 100.0, 40.0, 40.0),
            ],
            edge(detour()),
        );

        port_swap_to_l_shape(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests()).unwrap();

        assert_points(
            &layout.original_edges[0].points,
            &[(0.0, 20.0), (0.0, 80.0), (100.0, 80.0)],
        );
    }

    #[test]
    fn rejects_the_swap_when_every_candidate_source_segment_hits_a_node() {
        let original = detour();
        let mut layout = layout(
            vec![
                node("A", 0.0, 0.0, 40.0, 40.0),
                node("B", 100.0, 100.0, 40.0, 40.0),
                node("blocker", 0.0, 50.0, 40.0, 20.0),
            ],
            edge(original.clone()),
        );

        port_swap_to_l_shape(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests()).unwrap();

        assert_points(
            &layout.original_edges[0].points,
            &original
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
    }
}
