use super::*;
use crate::model::{LayoutPoint, SwimlaneDirection, SwimlaneTitleRect};
use crate::resources::{RenderResourcePolicy, ResourceLimitId};
use crate::swimlane::direction::LayoutWorkBudget;
use crate::swimlane::direction::materialized_geometry::common::{
    ReplacementMap, strict_crossing_count,
};
use crate::swimlane::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
use indexmap::IndexMap;

fn point(x: f64, y: f64) -> LayoutPoint {
    LayoutPoint { x, y }
}

fn node(id: &str, kind: WorkingNodeKind, x: f64, y: f64, width: f64, height: f64) -> WorkingNode {
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

fn group(
    id: &str,
    direction: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    title_rect: SwimlaneTitleRect,
) -> WorkingNode {
    let mut node = node(id, WorkingNodeKind::Group, x, y, width, height);
    node.requested_dir = Some(direction.to_string());
    node.title_rect = Some(title_rect);
    node
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

fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
    assert_eq!(actual.len(), expected.len(), "actual: {actual:?}");
    for (actual, &(x, y)) in actual.iter().zip(expected) {
        assert!((actual.x - x).abs() < 1e-9, "x: {} != {x}", actual.x);
        assert!((actual.y - y).abs() < 1e-9, "y: {} != {y}", actual.y);
    }
}

fn crossings(layout: &WorkingLayout) -> usize {
    strict_crossing_count(&layout.original_edges, &ReplacementMap::new())
}

fn edge_point_snapshot(layout: &WorkingLayout) -> Vec<Vec<(f64, f64)>> {
    layout
        .original_edges
        .iter()
        .map(|edge| edge.points.iter().map(|point| (point.x, point.y)).collect())
        .collect()
}

fn unbounded_work_budget() -> LayoutWorkBudget {
    LayoutWorkBudget::new(RenderResourcePolicy::unbounded_for_trusted_input(), 0).unwrap()
}

fn bounded_work_budget(max: usize) -> LayoutWorkBudget {
    let policy = RenderResourcePolicy::unbounded_for_trusted_input()
        .with_limit(ResourceLimitId::MaxLayoutWorkUnits, max)
        .unwrap();
    LayoutWorkBudget::new(policy, 0).unwrap()
}

#[test]
fn separates_shared_visible_terminal_rails_on_the_same_node_face() {
    let mut layout = layout(
        vec![
            node("A", WorkingNodeKind::Content, -40.0, -30.0, 10.0, 10.0),
            node("B", WorkingNodeKind::Content, 0.0, 0.0, 10.0, 80.0),
        ],
        vec![
            edge("A_B_1", "A", "B", vec![point(-30.0, 0.0), point(-5.0, 0.0)]),
            edge("A_B_2", "A", "B", vec![point(-30.0, 0.0), point(-5.0, 0.0)]),
        ],
    );

    separate_shared_rendered_terminal_lanes(&mut layout, &mut unbounded_work_budget()).unwrap();

    let mut terminal_y = layout
        .original_edges
        .iter()
        .map(|edge| edge.points.last().expect("terminal point").y)
        .collect::<Vec<_>>();
    terminal_y.sort_by(f64::total_cmp);
    assert_ne!(terminal_y[0], terminal_y[1]);
    assert!(terminal_y.contains(&0.0));
}

#[test]
fn terminal_lane_candidate_is_rejected_before_scanning_other_lanes() {
    let mut layout = layout(
        vec![
            node("A", WorkingNodeKind::Content, -40.0, -30.0, 10.0, 10.0),
            node("B", WorkingNodeKind::Content, 0.0, 0.0, 10.0, 80.0),
        ],
        vec![
            edge("A_B_1", "A", "B", vec![point(-30.0, 0.0), point(-5.0, 0.0)]),
            edge("A_B_2", "A", "B", vec![point(-30.0, 0.0), point(-5.0, 0.0)]),
        ],
    );
    let before = edge_point_snapshot(&layout);

    let error = separate_shared_rendered_terminal_lanes(&mut layout, &mut bounded_work_budget(8))
        .unwrap_err();
    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    assert_eq!(error.actual, 9);
    assert_eq!(error.max, 8);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn separates_near_parallel_terminal_rails_on_the_same_node_face() {
    let mut layout = layout(
        vec![
            node("A", WorkingNodeKind::Content, -40.0, -30.0, 10.0, 10.0),
            node("B", WorkingNodeKind::Content, 0.0, 0.0, 10.0, 60.0),
            node("C", WorkingNodeKind::Content, -40.0, 30.0, 10.0, 10.0),
        ],
        vec![
            edge("A_B", "A", "B", vec![point(-90.0, -4.0), point(-5.0, -4.0)]),
            edge("B_C", "B", "C", vec![point(-5.0, 4.0), point(-90.0, 4.0)]),
        ],
    );

    separate_shared_rendered_terminal_lanes(&mut layout, &mut unbounded_work_budget()).unwrap();

    let mut terminal_y = [
        layout.original_edges[0].points.last().unwrap().y,
        layout.original_edges[1].points[0].y,
    ];
    terminal_y.sort_by(f64::total_cmp);
    assert!(terminal_y[1] - terminal_y[0] >= 16.0);
}

#[test]
fn collapses_a_provably_redundant_rectangular_dogleg() {
    let mut layout = layout(
        Vec::new(),
        vec![edge(
            "A_B",
            "A",
            "B",
            vec![
                point(0.0, 0.0),
                point(10.0, 0.0),
                point(10.0, 10.0),
                point(0.0, 10.0),
                point(0.0, 20.0),
            ],
        )],
    );

    collapse_redundant_rectangular_doglegs(&mut layout, &mut unbounded_work_budget()).unwrap();

    assert_points(&layout.original_edges[0].points, &[(0.0, 0.0), (0.0, 20.0)]);
}

#[test]
fn dogleg_candidate_exhausts_budget_before_its_full_edge_scan() {
    let edges = (0..100)
        .map(|index| {
            let y = index as f64 * 100.0;
            edge(
                &format!("edge-{index}"),
                &format!("from-{index}"),
                &format!("to-{index}"),
                vec![
                    point(0.0, y),
                    point(10.0, y),
                    point(10.0, y + 10.0),
                    point(0.0, y + 10.0),
                    point(0.0, y + 20.0),
                ],
            )
        })
        .collect();
    let mut layout = layout(Vec::new(), edges);
    let before = edge_point_snapshot(&layout);
    let error = collapse_redundant_rectangular_doglegs(&mut layout, &mut bounded_work_budget(101))
        .unwrap_err();
    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };

    assert_eq!(error.actual, 200);
    assert_eq!(error.max, 101);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn lifts_a_same_side_rail_away_from_an_intervening_node_border() {
    let mut layout = layout(
        vec![
            node("26", WorkingNodeKind::Content, -166.0, 54.0, 232.0, 66.0),
            node("27", WorkingNodeKind::Content, 830.0, 54.0, 232.0, 66.0),
            node("28", WorkingNodeKind::Content, 166.0, 54.0, 232.0, 108.0),
        ],
        vec![edge(
            "L_26_27_0",
            "26",
            "27",
            vec![
                point(-166.0, 21.0),
                point(-166.0, 1.0),
                point(830.0, 1.0),
                point(830.0, 21.0),
            ],
        )],
    );

    lift_obstacle_hugging_same_side_rails(&mut layout, &mut unbounded_work_budget()).unwrap();

    assert_points(
        &layout.original_edges[0].points,
        &[
            (-166.0, 21.0),
            (-166.0, -20.0),
            (830.0, -20.0),
            (830.0, 21.0),
        ],
    );
}

#[test]
fn obstacle_rail_candidate_is_rejected_before_the_full_edge_scan() {
    let mut layout = layout(
        vec![
            node("26", WorkingNodeKind::Content, -166.0, 54.0, 232.0, 66.0),
            node("27", WorkingNodeKind::Content, 830.0, 54.0, 232.0, 66.0),
            node("28", WorkingNodeKind::Content, 166.0, 54.0, 232.0, 108.0),
        ],
        vec![
            edge(
                "L_26_27_0",
                "26",
                "27",
                vec![
                    point(-166.0, 21.0),
                    point(-166.0, 1.0),
                    point(830.0, 1.0),
                    point(830.0, 21.0),
                ],
            ),
            edge(
                "far",
                "",
                "",
                vec![point(2_000.0, 0.0), point(2_000.0, 10.0)],
            ),
        ],
    );
    let before = edge_point_snapshot(&layout);

    let error = lift_obstacle_hugging_same_side_rails(&mut layout, &mut bounded_work_budget(25))
        .unwrap_err();
    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    assert_eq!(error.actual, 26);
    assert_eq!(error.max, 25);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn raises_top_lane_title_bands_above_a_clear_same_side_rail() {
    let mut layout = layout(
        vec![
            node("26", WorkingNodeKind::Content, -166.0, 54.0, 232.0, 66.0),
            node("27", WorkingNodeKind::Content, 830.0, 54.0, 232.0, 66.0),
            node("28", WorkingNodeKind::Content, 166.0, 54.0, 232.0, 108.0),
            group(
                "General_Manager",
                "TD",
                166.0,
                54.0,
                996.0,
                180.0,
                SwimlaneTitleRect {
                    left: -332.0,
                    right: 664.0,
                    top: -36.0,
                    bottom: -15.0,
                },
            ),
        ],
        vec![edge(
            "L_26_27_0",
            "26",
            "27",
            vec![
                point(-166.0, 21.0),
                point(-166.0, 1.0),
                point(830.0, 1.0),
                point(830.0, 21.0),
            ],
        )],
    );

    lift_obstacle_hugging_same_side_rails(&mut layout, &mut unbounded_work_budget()).unwrap();
    lift_top_lane_title_bands_above_rails(&mut layout, &mut unbounded_work_budget()).unwrap();

    let lane = &layout.nodes["General_Manager"];
    assert_eq!(lane.y, 49.5);
    assert_eq!(lane.height, 189.0);
    let title = lane.title_rect.as_ref().unwrap();
    assert_eq!(
        (title.left, title.right, title.top, title.bottom),
        (-332.0, 664.0, -45.0, -24.0)
    );
}

#[test]
fn shifts_lr_lane_title_bands_left_of_a_clear_left_side_rail() {
    let title = |top, bottom| SwimlaneTitleRect {
        left: -100.0,
        right: -64.0,
        top,
        bottom,
    };
    let mut layout = layout(
        vec![
            group("BOD", "LR", 0.0, 100.0, 200.0, 200.0, title(0.0, 200.0)),
            group(
                "Finance_Head",
                "LR",
                0.0,
                300.0,
                200.0,
                200.0,
                title(200.0, 400.0),
            ),
        ],
        vec![edge(
            "L_27_28_0",
            "27",
            "28",
            vec![
                point(-80.0, 350.0),
                point(-80.0, 300.0),
                point(-90.0, 300.0),
                point(-90.0, 50.0),
                point(-80.0, 50.0),
            ],
        )],
    );

    shift_left_lane_title_bands_left_of_rails(&mut layout, &mut unbounded_work_budget()).unwrap();

    for id in ["BOD", "Finance_Head"] {
        assert_eq!(layout.nodes[id].x, -15.0);
        assert_eq!(layout.nodes[id].width, 230.0);
    }
    let bod = layout.nodes["BOD"].title_rect.as_ref().unwrap();
    assert_eq!(
        (bod.left, bod.right, bod.top, bod.bottom),
        (-130.0, -94.0, 0.0, 200.0)
    );
}

#[test]
fn lane_title_scan_is_rejected_before_mutating_layout() {
    let title = |top, bottom| SwimlaneTitleRect {
        left: -100.0,
        right: -64.0,
        top,
        bottom,
    };
    let mut layout = layout(
        vec![
            group("BOD", "LR", 0.0, 100.0, 200.0, 200.0, title(0.0, 200.0)),
            group(
                "Finance_Head",
                "LR",
                0.0,
                300.0,
                200.0,
                200.0,
                title(200.0, 400.0),
            ),
        ],
        vec![edge(
            "L_27_28_0",
            "27",
            "28",
            vec![point(-80.0, 350.0), point(-80.0, 50.0)],
        )],
    );
    let before_nodes = layout
        .nodes
        .values()
        .map(|node| {
            (
                node.id.clone(),
                node.x,
                node.y,
                node.width,
                node.height,
                node.title_rect
                    .as_ref()
                    .map(|rect| (rect.left, rect.right, rect.top, rect.bottom)),
            )
        })
        .collect::<Vec<_>>();
    let before_edges = layout
        .original_edges
        .iter()
        .map(|edge| {
            edge.points
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut budget = bounded_work_budget(1);

    let error = shift_left_lane_title_bands_left_of_rails(&mut layout, &mut budget).unwrap_err();

    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    assert_eq!(error.actual, 2);
    assert_eq!(error.max, 1);
    assert_eq!(budget.used(), 0);
    assert_eq!(
        layout
            .nodes
            .values()
            .map(|node| {
                (
                    node.id.clone(),
                    node.x,
                    node.y,
                    node.width,
                    node.height,
                    node.title_rect
                        .as_ref()
                        .map(|rect| (rect.left, rect.right, rect.top, rect.bottom)),
                )
            })
            .collect::<Vec<_>>(),
        before_nodes
    );
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
        before_edges
    );
}

#[test]
fn swaps_shared_destination_terminal_tails_when_both_swaps_remove_a_crossing() {
    let top_port = point(297.553125, 1171.1287841796875);
    let right_port = point(318.890625, 1196.6287841796875);
    let mut layout = layout(
        vec![
            node(
                "C",
                WorkingNodeKind::Content,
                292.21875,
                287.5,
                150.05624389648438,
                91.0,
            ),
            node(
                "D",
                WorkingNodeKind::Content,
                292.21875,
                570.5,
                94.11250305175781,
                91.0,
            ),
            node(
                "exit",
                WorkingNodeKind::Content,
                292.21875,
                1196.6287841796875,
                53.34375,
                51.0,
            ),
        ],
        vec![
            edge(
                "L_C_exit_0",
                "C",
                "exit",
                vec![
                    point(367.2468719482422, 287.5),
                    point(387.2468719482422, 287.5),
                    point(387.2468719482422, 1151.1287841796875),
                    point(297.553125, 1151.1287841796875),
                    top_port.clone(),
                ],
            ),
            edge(
                "L_D_exit_0",
                "D",
                "exit",
                vec![
                    point(339.2750015258789, 570.5),
                    point(359.2750015258789, 570.5),
                    point(359.2750015258789, 1196.6287841796875),
                    right_port.clone(),
                ],
            ),
        ],
    );
    assert_eq!(crossings(&layout), 1);

    swap_destination_terminal_tails_to_reduce_crossings(&mut layout, &mut unbounded_work_budget())
        .unwrap();

    assert_eq!(crossings(&layout), 0);
    assert_points(
        layout.original_edges[0]
            .points
            .last()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
            .as_slice(),
        &[(right_port.x, right_port.y)],
    );
    assert_points(
        layout.original_edges[1]
            .points
            .last()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>()
            .as_slice(),
        &[(top_port.x, top_port.y)],
    );
}

#[test]
fn terminal_tail_second_candidate_scan_is_rejected_at_the_cumulative_boundary() {
    let top_port = point(297.553125, 1171.1287841796875);
    let right_port = point(318.890625, 1196.6287841796875);
    let mut layout = layout(
        vec![
            node(
                "C",
                WorkingNodeKind::Content,
                292.21875,
                287.5,
                150.05624389648438,
                91.0,
            ),
            node(
                "D",
                WorkingNodeKind::Content,
                292.21875,
                570.5,
                94.11250305175781,
                91.0,
            ),
            node(
                "exit",
                WorkingNodeKind::Content,
                292.21875,
                1196.6287841796875,
                53.34375,
                51.0,
            ),
        ],
        vec![
            edge(
                "L_C_exit_0",
                "C",
                "exit",
                vec![
                    point(367.2468719482422, 287.5),
                    point(387.2468719482422, 287.5),
                    point(387.2468719482422, 1151.1287841796875),
                    point(297.553125, 1151.1287841796875),
                    top_port,
                ],
            ),
            edge(
                "L_D_exit_0",
                "D",
                "exit",
                vec![
                    point(339.2750015258789, 570.5),
                    point(359.2750015258789, 570.5),
                    point(359.2750015258789, 1196.6287841796875),
                    right_port,
                ],
            ),
        ],
    );
    assert!(crossings(&layout) > 0);
    let before = edge_point_snapshot(&layout);
    let mut budget = bounded_work_budget(37);
    let error =
        swap_destination_terminal_tails_to_reduce_crossings(&mut layout, &mut budget).unwrap_err();

    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    // The first candidate consumes the exact boundary. The second candidate has four
    // segments and three node rectangles, so its next scan requests 12 more units.
    assert_eq!(budget.used(), 37);
    assert_eq!(error.actual, 49);
    assert_eq!(error.max, 37);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn reassigns_overlapping_external_rail_channels_as_a_crossing_minimizing_bundle() {
    let mut layout = layout(
        vec![
            node("26", WorkingNodeKind::Content, -22.0, 664.0, 116.0, 64.0),
            node("27", WorkingNodeKind::Content, -22.0, 1602.0, 116.0, 116.0),
            node("28", WorkingNodeKind::Content, -22.0, 1050.0, 116.0, 80.0),
        ],
        vec![
            edge(
                "L_26_27_0",
                "26",
                "27",
                vec![
                    point(-80.0, 664.0),
                    point(-100.0, 664.0),
                    point(-100.0, 1660.0),
                    point(-80.0, 1660.0),
                ],
            ),
            edge(
                "L_27_28_0",
                "27",
                "28",
                vec![
                    point(36.0, 1544.0),
                    point(65.0, 1544.0),
                    point(65.0, 1467.0),
                    point(-126.4, 1467.0),
                    point(-126.4, 1132.0),
                    point(36.0, 1132.0),
                    point(36.0, 1050.0),
                ],
            ),
        ],
    );
    assert_eq!(crossings(&layout), 2);

    reassign_crossing_external_rail_channels(&mut layout, &mut unbounded_work_budget()).unwrap();

    assert_eq!(crossings(&layout), 0);
    assert_eq!(layout.original_edges[0].points[1].x, -126.4);
    assert_eq!(layout.original_edges[1].points[3].x, -100.0);
}

fn shortcut_fixture(with_nodes: bool) -> WorkingLayout {
    layout(
        if with_nodes {
            vec![
                node("23", WorkingNodeKind::Content, 36.0, 1328.0, 232.0, 108.0),
                node("27", WorkingNodeKind::Content, 36.0, 1660.0, 232.0, 66.0),
                node("28", WorkingNodeKind::Content, 36.0, 996.0, 232.0, 108.0),
            ]
        } else {
            Vec::new()
        },
        vec![edge(
            if with_nodes {
                "L_27_28_0"
            } else {
                "missing_endpoint_edge"
            },
            if with_nodes { "27" } else { "missing-start" },
            if with_nodes { "28" } else { "missing-end" },
            vec![
                point(36.0, 1544.0),
                point(65.0, 1544.0),
                point(65.0, 1467.0),
                point(-100.0, 1467.0),
                point(-100.0, 1132.0),
                point(36.0, 1132.0),
                point(36.0, 1050.0),
            ],
        )],
    )
}

#[test]
fn shortcuts_a_dominated_orthogonal_jog_when_clearance_is_preserved() {
    let mut layout = shortcut_fixture(true);
    shortcut_redundant_orthogonal_jogs(&mut layout, &mut unbounded_work_budget()).unwrap();
    assert_points(
        &layout.original_edges[0].points,
        &[
            (36.0, 1544.0),
            (36.0, 1467.0),
            (-100.0, 1467.0),
            (-100.0, 1132.0),
            (36.0, 1132.0),
            (36.0, 1050.0),
        ],
    );
}

#[test]
fn shortcuts_a_redundant_jog_when_endpoint_nodes_are_unavailable() {
    let mut layout = shortcut_fixture(false);
    shortcut_redundant_orthogonal_jogs(&mut layout, &mut unbounded_work_budget()).unwrap();
    assert_points(
        &layout.original_edges[0].points,
        &[
            (36.0, 1544.0),
            (65.0, 1544.0),
            (65.0, 1132.0),
            (36.0, 1132.0),
            (36.0, 1050.0),
        ],
    );
}

#[test]
fn jog_candidate_scan_is_rejected_before_mutating_edges() {
    let mut layout = shortcut_fixture(false);
    layout.original_edges.extend((0..10).map(|index| {
        edge(
            &format!("straight-{index}"),
            "",
            "",
            vec![
                point(1_000.0 + index as f64 * 10.0, 0.0),
                point(1_000.0 + index as f64 * 10.0, 10.0),
            ],
        )
    }));
    let before = edge_point_snapshot(&layout);
    let error =
        shortcut_redundant_orthogonal_jogs(&mut layout, &mut bounded_work_budget(67)).unwrap_err();

    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    assert_eq!(error.actual, 77);
    assert_eq!(error.max, 67);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn resolves_a_rendered_orthogonal_crossing_with_source_ordered_candidates() {
    let mut layout = layout(
        vec![
            node("A", WorkingNodeKind::Content, -50.0, 0.0, 10.0, 10.0),
            node("B", WorkingNodeKind::Content, 50.0, 0.0, 10.0, 10.0),
            node("C", WorkingNodeKind::Content, 0.0, -50.0, 10.0, 10.0),
            node("D", WorkingNodeKind::Content, 0.0, 50.0, 10.0, 10.0),
        ],
        vec![
            edge("A_B", "A", "B", vec![point(-45.0, 0.0), point(45.0, 0.0)]),
            edge("C_D", "C", "D", vec![point(0.0, -45.0), point(0.0, 45.0)]),
        ],
    );
    assert_eq!(crossings(&layout), 1);

    resolve_rendered_orthogonal_crossings(&mut layout, &mut unbounded_work_budget()).unwrap();

    assert_eq!(crossings(&layout), 0);
}

#[test]
fn crossing_candidate_scan_is_rejected_before_mutating_edges() {
    let mut layout = layout(
        vec![
            node("A", WorkingNodeKind::Content, -50.0, 0.0, 10.0, 10.0),
            node("B", WorkingNodeKind::Content, 50.0, 0.0, 10.0, 10.0),
            node("C", WorkingNodeKind::Content, 0.0, -50.0, 10.0, 10.0),
            node("D", WorkingNodeKind::Content, 0.0, 50.0, 10.0, 10.0),
        ],
        vec![
            edge("A_B", "A", "B", vec![point(-45.0, 0.0), point(45.0, 0.0)]),
            edge("C_D", "C", "D", vec![point(0.0, -45.0), point(0.0, 45.0)]),
        ],
    );
    let before = edge_point_snapshot(&layout);
    let error = resolve_rendered_orthogonal_crossings(&mut layout, &mut bounded_work_budget(22))
        .unwrap_err();

    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };
    assert!(error.actual > 22);
    assert_eq!(error.max, 22);
    assert_eq!(edge_point_snapshot(&layout), before);
}

#[test]
fn dense_crossing_candidate_bound_is_charged_after_the_snapshot_and_before_scanning() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for index in 0..4 {
        let y = -15.0 + index as f64 * 10.0;
        let from = format!("H{index}L");
        let to = format!("H{index}R");
        nodes.push(node(&from, WorkingNodeKind::Content, -50.0, y, 10.0, 10.0));
        nodes.push(node(&to, WorkingNodeKind::Content, 50.0, y, 10.0, 10.0));
        edges.push(edge(
            &format!("H{index}"),
            &from,
            &to,
            vec![point(-45.0, y), point(45.0, y)],
        ));
    }
    for index in 0..4 {
        let x = -15.0 + index as f64 * 10.0;
        let from = format!("V{index}T");
        let to = format!("V{index}B");
        nodes.push(node(&from, WorkingNodeKind::Content, x, -50.0, 10.0, 10.0));
        nodes.push(node(&to, WorkingNodeKind::Content, x, 50.0, 10.0, 10.0));
        edges.push(edge(
            &format!("V{index}"),
            &from,
            &to,
            vec![point(x, -45.0), point(x, 45.0)],
        ));
    }
    let mut layout = layout(nodes, edges);
    assert_eq!(crossings(&layout), 16);
    let before = edge_point_snapshot(&layout);
    let error = resolve_rendered_orthogonal_crossings(&mut layout, &mut bounded_work_budget(109))
        .unwrap_err();
    let crate::Error::ResourceLimitExceeded(error) = error else {
        panic!("expected max_layout_work_units resource limit error");
    };

    assert!(error.actual > 109);
    assert_eq!(error.max, 109);
    assert_eq!(edge_point_snapshot(&layout), before);
}
