use super::super::working::{WorkingEdge, WorkingLayout, WorkingNodeKind};
use super::LayoutWorkBudget;
use super::geometry::{
    classify_three_segment_route, collect_real_node_bounds, get_node_pair_geometry,
    segment_conflicts_with_any_edge, segment_hits_any_rect,
};
use crate::Result;
use crate::model::LayoutPoint;
use crate::swimlane::config::EPSILON;
use std::collections::HashMap;

const MINIMUM_PORT_SPACING: f64 = 8.0;
const PORT_SHIFT: f64 = MINIMUM_PORT_SPACING / 2.0;
const LABEL_CLEARANCE_BUFFER: f64 = 3.0;

#[derive(Debug, Clone, Copy)]
struct LabelDimensions {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy)]
enum ShiftAxis {
    X,
    Y,
}

fn pair_key<'a>(first: &'a str, second: &'a str) -> (&'a str, &'a str) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

fn label_clearance_for(
    edges: &[WorkingEdge],
    edge_index: usize,
    label_dimensions_by_id: &HashMap<&str, LabelDimensions>,
    source_id: &str,
    destination_id: &str,
    axis: ShiftAxis,
    work_budget: &mut LayoutWorkBudget,
) -> Result<f64> {
    let target_pair = pair_key(source_id, destination_id);
    let mut maximum_half_extent: f64 = 0.0;
    work_budget.charge(edges.len())?;
    for (other_index, edge) in edges.iter().enumerate() {
        if other_index != edge_index && pair_key(&edge.from, &edge.to) != target_pair {
            continue;
        }
        let Some(label_node_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some(dimensions) = label_dimensions_by_id.get(label_node_id) else {
            continue;
        };
        let half_extent = match axis {
            ShiftAxis::X => dimensions.width / 2.0,
            ShiftAxis::Y => dimensions.height / 2.0,
        };
        maximum_half_extent = maximum_half_extent.max(half_extent);
    }
    if maximum_half_extent > 0.0 {
        Ok(maximum_half_extent + LABEL_CLEARANCE_BUFFER)
    } else {
        Ok(0.0)
    }
}

/// Replaces an eligible four-point sibling detour with a clear straight route.
pub(super) fn straighten_collinear_sibling_detours(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (node_info_by_id, real_node_rects) = collect_real_node_bounds(layout);
    work_budget.charge(layout.nodes.len())?;
    let label_dimensions_by_id = layout
        .nodes
        .values()
        .filter(|node| node.kind == WorkingNodeKind::EdgeLabel)
        .map(|node| {
            (
                node.id.as_str(),
                LabelDimensions {
                    width: node.width,
                    height: node.height,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    work_budget.charge(layout.original_edges.len())?;
    for edge_index in 0..layout.original_edges.len() {
        let edge = layout.original_edges[edge_index].clone();
        if classify_three_segment_route(&edge.points, EPSILON).is_none() {
            continue;
        }
        let Some(node_pair) = get_node_pair_geometry(&edge, &node_info_by_id, EPSILON) else {
            continue;
        };
        if node_pair.collinear_x == node_pair.collinear_y {
            continue;
        }

        let (target_source, target_destination, shift_axis) = if node_pair.collinear_x {
            let destination_below = node_pair.dst_info.cy > node_pair.src_info.cy;
            (
                LayoutPoint {
                    x: node_pair.src_info.cx,
                    y: if destination_below {
                        node_pair.src_info.rect.bottom
                    } else {
                        node_pair.src_info.rect.top
                    },
                },
                LayoutPoint {
                    x: node_pair.dst_info.cx,
                    y: if destination_below {
                        node_pair.dst_info.rect.top
                    } else {
                        node_pair.dst_info.rect.bottom
                    },
                },
                ShiftAxis::X,
            )
        } else {
            let destination_east = node_pair.dst_info.cx > node_pair.src_info.cx;
            (
                LayoutPoint {
                    x: if destination_east {
                        node_pair.src_info.rect.right
                    } else {
                        node_pair.src_info.rect.left
                    },
                    y: node_pair.src_info.cy,
                },
                LayoutPoint {
                    x: if destination_east {
                        node_pair.dst_info.rect.left
                    } else {
                        node_pair.dst_info.rect.right
                    },
                    y: node_pair.dst_info.cy,
                },
                ShiftAxis::Y,
            )
        };
        let excluded_node_ids = [node_pair.src_id.as_str(), node_pair.dst_id.as_str()];
        work_budget.charge(real_node_rects.len())?;
        if segment_hits_any_rect(
            &target_source,
            &target_destination,
            &real_node_rects,
            &excluded_node_ids,
            1.0,
        ) {
            continue;
        }

        let label_shift = label_clearance_for(
            &layout.original_edges,
            edge_index,
            &label_dimensions_by_id,
            &node_pair.src_id,
            &node_pair.dst_id,
            shift_axis,
            work_budget,
        )?;
        let effective_shift = label_shift.max(PORT_SHIFT);
        for delta in [0.0, effective_shift, -effective_shift] {
            let mut shifted_source = target_source.clone();
            let mut shifted_destination = target_destination.clone();
            match shift_axis {
                ShiftAxis::X => {
                    shifted_source.x += delta;
                    shifted_destination.x += delta;
                    if shifted_source.x <= node_pair.src_info.rect.left
                        || shifted_source.x >= node_pair.src_info.rect.right
                        || shifted_destination.x <= node_pair.dst_info.rect.left
                        || shifted_destination.x >= node_pair.dst_info.rect.right
                    {
                        continue;
                    }
                }
                ShiftAxis::Y => {
                    shifted_source.y += delta;
                    shifted_destination.y += delta;
                    if shifted_source.y <= node_pair.src_info.rect.top
                        || shifted_source.y >= node_pair.src_info.rect.bottom
                        || shifted_destination.y <= node_pair.dst_info.rect.top
                        || shifted_destination.y >= node_pair.dst_info.rect.bottom
                    {
                        continue;
                    }
                }
            }

            work_budget.charge(1)?;
            work_budget.charge(real_node_rects.len())?;
            if segment_hits_any_rect(
                &shifted_source,
                &shifted_destination,
                &real_node_rects,
                &excluded_node_ids,
                1.0,
            ) {
                continue;
            }
            work_budget.charge(layout.original_edges.len())?;
            if segment_conflicts_with_any_edge(
                &shifted_source,
                &shifted_destination,
                &layout.original_edges,
                Some(edge.id.as_str()),
                EPSILON,
                false,
            ) {
                continue;
            }

            layout.original_edges[edge_index].points = vec![shifted_source, shifted_destination];
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::working::{WorkingNode, WorkingNodeKind};
    use super::*;
    use crate::model::SwimlaneDirection;
    use indexmap::IndexMap;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
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

    fn content_node(id: &str, x: f64, y: f64) -> WorkingNode {
        node(id, WorkingNodeKind::Content, x, y, 40.0, 40.0)
    }

    fn edge(
        id: &str,
        from: &str,
        to: &str,
        label_node_id: Option<&str>,
        points: Vec<LayoutPoint>,
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

    fn vertical_detour() -> Vec<LayoutPoint> {
        vec![
            point(0.0, 20.0),
            point(-30.0, 20.0),
            point(-30.0, 80.0),
            point(0.0, 80.0),
        ]
    }

    fn horizontal_detour() -> Vec<LayoutPoint> {
        vec![
            point(20.0, 0.0),
            point(20.0, -30.0),
            point(80.0, -30.0),
            point(80.0, 0.0),
        ]
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, &(x, y)) in actual.iter().zip(expected) {
            assert!((actual.x - x).abs() < EPSILON, "x: {} != {x}", actual.x);
            assert!((actual.y - y).abs() < EPSILON, "y: {} != {y}", actual.y);
        }
    }

    fn assert_unchanged(actual: &[LayoutPoint], expected: &[LayoutPoint]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual.x - expected.x).abs() < EPSILON,
                "x: {} != {}",
                actual.x,
                expected.x
            );
            assert!(
                (actual.y - expected.y).abs() < EPSILON,
                "y: {} != {}",
                actual.y,
                expected.y
            );
        }
    }

    #[test]
    fn centers_a_clear_collinear_route_like_the_upstream_simple_two_regression() {
        let mut layout = layout(
            vec![content_node("A", 0.0, 0.0), content_node("B", 0.0, 100.0)],
            vec![edge("A_B", "A", "B", None, vertical_detour())],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_points(
            &layout.original_edges[0].points,
            &[(0.0, 20.0), (0.0, 80.0)],
        );
    }

    #[test]
    fn shifts_by_half_the_minimum_port_spacing_when_the_center_axis_is_claimed() {
        let mut layout = layout(
            vec![content_node("A", 0.0, 0.0), content_node("B", 0.0, 100.0)],
            vec![
                edge(
                    "primary",
                    "A",
                    "B",
                    None,
                    vec![point(0.0, 20.0), point(0.0, 80.0)],
                ),
                edge("detour", "A", "B", None, vertical_detour()),
            ],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_points(
            &layout.original_edges[1].points,
            &[(4.0, 20.0), (4.0, 80.0)],
        );
    }

    #[test]
    fn uses_an_antiparallel_sibling_label_width_as_vertical_route_clearance() {
        let mut layout = layout(
            vec![
                content_node("A", 0.0, 0.0),
                content_node("B", 0.0, 100.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 50.0, 20.0, 8.0),
            ],
            vec![
                edge(
                    "primary",
                    "B",
                    "A",
                    Some("label"),
                    vec![point(0.0, 80.0), point(0.0, 20.0)],
                ),
                edge("detour", "A", "B", None, vertical_detour()),
            ],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_points(
            &layout.original_edges[1].points,
            &[(13.0, 20.0), (13.0, 80.0)],
        );
    }

    #[test]
    fn uses_label_height_as_horizontal_route_clearance() {
        let mut layout = layout(
            vec![
                content_node("A", 0.0, 0.0),
                content_node("B", 100.0, 0.0),
                node("label", WorkingNodeKind::EdgeLabel, 50.0, 0.0, 8.0, 14.0),
            ],
            vec![
                edge(
                    "primary",
                    "B",
                    "A",
                    Some("label"),
                    vec![point(80.0, 0.0), point(20.0, 0.0)],
                ),
                edge("detour", "A", "B", None, horizontal_detour()),
            ],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_points(
            &layout.original_edges[1].points,
            &[(20.0, 10.0), (80.0, 10.0)],
        );
    }

    #[test]
    fn tries_the_negative_shift_when_the_positive_shift_crosses_an_edge() {
        let mut layout = layout(
            vec![content_node("A", 0.0, 0.0), content_node("B", 0.0, 100.0)],
            vec![
                edge(
                    "primary",
                    "A",
                    "B",
                    None,
                    vec![point(0.0, 20.0), point(0.0, 80.0)],
                ),
                edge(
                    "crossing",
                    "missing_source",
                    "missing_destination",
                    None,
                    vec![point(1.0, 50.0), point(10.0, 50.0)],
                ),
                edge("detour", "A", "B", None, vertical_detour()),
            ],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_points(
            &layout.original_edges[2].points,
            &[(-4.0, 20.0), (-4.0, 80.0)],
        );
    }

    #[test]
    fn preserves_the_detour_when_a_real_node_blocks_the_centered_route() {
        let original = vertical_detour();
        let mut layout = layout(
            vec![
                content_node("A", 0.0, 0.0),
                content_node("B", 0.0, 100.0),
                content_node("blocker", 0.0, 50.0),
            ],
            vec![edge("detour", "A", "B", None, original.clone())],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_unchanged(&layout.original_edges[0].points, &original);
    }

    #[test]
    fn preserves_the_detour_when_label_clearance_does_not_fit_on_the_shared_face() {
        let original = vertical_detour();
        let mut layout = layout(
            vec![
                content_node("A", 0.0, 0.0),
                content_node("B", 0.0, 100.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 50.0, 40.0, 8.0),
            ],
            vec![
                edge(
                    "primary",
                    "B",
                    "A",
                    Some("label"),
                    vec![point(0.0, 80.0), point(0.0, 20.0)],
                ),
                edge("detour", "A", "B", None, original.clone()),
            ],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_unchanged(&layout.original_edges[1].points, &original);
    }

    #[test]
    fn ignores_a_three_segment_route_between_non_collinear_nodes() {
        let original = vec![
            point(20.0, 0.0),
            point(50.0, 0.0),
            point(50.0, 100.0),
            point(80.0, 100.0),
        ];
        let mut layout = layout(
            vec![content_node("A", 0.0, 0.0), content_node("B", 100.0, 100.0)],
            vec![edge("detour", "A", "B", None, original.clone())],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_unchanged(&layout.original_edges[0].points, &original);
    }

    #[test]
    fn ignores_routes_that_are_not_exactly_three_orthogonal_segments() {
        let original = vec![
            point(0.0, 20.0),
            point(-30.0, 20.0),
            point(-30.0, 50.0),
            point(-20.0, 50.0),
            point(-20.0, 80.0),
            point(0.0, 80.0),
        ];
        let mut layout = layout(
            vec![content_node("A", 0.0, 0.0), content_node("B", 0.0, 100.0)],
            vec![edge("detour", "A", "B", None, original.clone())],
        );

        straighten_collinear_sibling_detours(
            &mut layout,
            &mut LayoutWorkBudget::unbounded_for_tests(),
        )
        .unwrap();

        assert_unchanged(&layout.original_edges[0].points, &original);
    }
}
