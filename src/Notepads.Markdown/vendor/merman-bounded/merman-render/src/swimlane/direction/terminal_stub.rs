use super::super::working::WorkingLayout;
use super::LayoutWorkBudget;
use super::geometry::{
    EPSILON, RectEntry, collect_node_rect_entries, dedupe_consecutive_points,
    is_horizontal_segment, is_vertical_segment, orthogonal_segments_strictly_cross,
    rect_of_node_bounds, same_x, same_y, segment_hits_any_rect,
};
use crate::Result;
use crate::model::LayoutPoint;
use std::collections::HashSet;

const MINIMUM_STUB_LENGTH: f64 = 10.0;
const OBSTACLE_BUFFER: f64 = 2.0;

#[derive(Debug)]
struct StubRewrite {
    points: Vec<LayoutPoint>,
    label_anchor: Option<(String, LayoutPoint)>,
}

fn own_segment_key(start: &LayoutPoint, end: &LayoutPoint) -> String {
    format!("{:.3},{:.3}|{:.3},{:.3}", start.x, start.y, end.x, end.y)
}

fn segment_crosses_other_edge(
    layout: &WorkingLayout,
    edge_index: usize,
    start: &LayoutPoint,
    end: &LayoutPoint,
    own_segments: &HashSet<String>,
    work_budget: &mut LayoutWorkBudget,
) -> Result<bool> {
    work_budget.charge(layout.original_edges.len().saturating_sub(1))?;
    Ok(layout
        .original_edges
        .iter()
        .enumerate()
        .filter(|(other_index, _)| *other_index != edge_index)
        .flat_map(|(_, edge)| edge.points.windows(2))
        .any(|segment| {
            !own_segments.contains(&own_segment_key(&segment[0], &segment[1]))
                && orthogonal_segments_strictly_cross(start, end, &segment[0], &segment[1], EPSILON)
        }))
}

fn label_anchor_for_rewrite(
    layout: &WorkingLayout,
    label_node_id: Option<&str>,
    points: &[LayoutPoint],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<(String, LayoutPoint)>> {
    let Some(label_node_id) = label_node_id else {
        return Ok(None);
    };
    let Some(label) = layout.nodes.get(label_node_id) else {
        return Ok(None);
    };
    let label_width = label.width;
    let label_height = label.height;
    if label_width <= 0.0 || label_height <= 0.0 {
        return Ok(None);
    }

    let mut best = None;
    let mut best_length = -1.0;
    work_budget.charge(points.len().saturating_sub(1))?;
    for segment in points.windows(2) {
        let start = &segment[0];
        let end = &segment[1];
        let length = (end.x - start.x).hypot(end.y - start.y);
        let horizontal = same_y(start, end, EPSILON);
        let vertical = same_x(start, end, EPSILON);
        let fits = (horizontal && length >= label_width + 2.0)
            || (vertical && length >= label_height + 2.0);
        if fits && length > best_length {
            best_length = length;
            best = Some(LayoutPoint {
                x: (start.x + end.x) / 2.0,
                y: (start.y + end.y) / 2.0,
            });
        }
    }
    Ok(best.map(|anchor| (label_node_id.to_string(), anchor)))
}

fn rewrite_for_edge(
    layout: &WorkingLayout,
    edge_index: usize,
    real_node_rects: &[RectEntry],
    label_rects: &[RectEntry],
    work_budget: &mut LayoutWorkBudget,
) -> Result<Option<StubRewrite>> {
    let edge = &layout.original_edges[edge_index];
    if edge.points.len() < 4 {
        return Ok(None);
    }
    let points = dedupe_consecutive_points(&edge.points, EPSILON);
    if points.len() < 4 {
        return Ok(None);
    }

    let last = points.len() - 1;
    let end = &points[last];
    let penultimate = &points[last - 1];
    let previous = &points[last - 2];
    let last_delta_x = end.x - penultimate.x;
    let last_delta_y = end.y - penultimate.y;
    let last_length = last_delta_x.hypot(last_delta_y);
    if !(EPSILON..MINIMUM_STUB_LENGTH).contains(&last_length) {
        return Ok(None);
    }

    let penultimate_delta_x = penultimate.x - previous.x;
    let penultimate_delta_y = penultimate.y - previous.y;
    if penultimate_delta_x.hypot(penultimate_delta_y) < EPSILON {
        return Ok(None);
    }

    let last_horizontal = is_horizontal_segment(penultimate, end, EPSILON);
    let last_vertical = is_vertical_segment(penultimate, end, EPSILON);
    let penultimate_horizontal = is_horizontal_segment(previous, penultimate, EPSILON);
    let penultimate_vertical = is_vertical_segment(previous, penultimate, EPSILON);
    if !((last_horizontal && penultimate_vertical) || (last_vertical && penultimate_horizontal)) {
        return Ok(None);
    }

    let Some(destination) = layout.nodes.get(&edge.to) else {
        return Ok(None);
    };
    let Some(destination_rect) = rect_of_node_bounds(destination) else {
        return Ok(None);
    };
    let (new_previous, new_end) = if penultimate_vertical {
        let approach_from_below = penultimate_delta_y < 0.0;
        (
            LayoutPoint {
                x: destination.x,
                y: previous.y,
            },
            LayoutPoint {
                x: destination.x,
                y: if approach_from_below {
                    destination_rect.bottom
                } else {
                    destination_rect.top
                },
            },
        )
    } else {
        let approach_from_left = penultimate_delta_x > 0.0;
        (
            LayoutPoint {
                x: previous.x,
                y: destination.y,
            },
            LayoutPoint {
                x: if approach_from_left {
                    destination_rect.right
                } else {
                    destination_rect.left
                },
                y: destination.y,
            },
        )
    };

    work_budget.charge(1)?;
    work_budget.charge(real_node_rects.len())?;
    if segment_hits_any_rect(
        &new_previous,
        &new_end,
        real_node_rects,
        &[edge.to.as_str()],
        -OBSTACLE_BUFFER,
    ) {
        return Ok(None);
    }
    work_budget.charge(label_rects.len())?;
    if segment_hits_any_rect(&new_previous, &new_end, label_rects, &[], -OBSTACLE_BUFFER) {
        return Ok(None);
    }

    if let Some(source) = layout.nodes.get(&edge.from)
        && let Some(source_rect) = rect_of_node_bounds(source)
        && source_rect.contains_point(&new_previous, OBSTACLE_BUFFER)
    {
        return Ok(None);
    }

    work_budget.charge(points.len().saturating_sub(1))?;
    let own_segments = points
        .windows(2)
        .map(|segment| own_segment_key(&segment[0], &segment[1]))
        .collect::<HashSet<_>>();
    if segment_crosses_other_edge(
        layout,
        edge_index,
        &new_previous,
        &new_end,
        &own_segments,
        work_budget,
    )? {
        return Ok(None);
    }

    let before_previous = &points[last - 3];
    let endpoint_ids = [edge.from.as_str(), edge.to.as_str()];
    work_budget.charge(real_node_rects.len())?;
    if segment_hits_any_rect(
        before_previous,
        &new_previous,
        real_node_rects,
        &endpoint_ids,
        -OBSTACLE_BUFFER,
    ) {
        return Ok(None);
    }
    if segment_crosses_other_edge(
        layout,
        edge_index,
        before_previous,
        &new_previous,
        &own_segments,
        work_budget,
    )? {
        return Ok(None);
    }

    let mut rewritten = points[..last - 2].to_vec();
    rewritten.push(new_previous);
    rewritten.push(new_end);
    let label_anchor = label_anchor_for_rewrite(
        layout,
        edge.label_node_id.as_deref(),
        &rewritten,
        work_budget,
    )?;
    Ok(Some(StubRewrite {
        points: rewritten,
        label_anchor,
    }))
}

pub(super) fn collapse_short_terminal_stub(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    work_budget.charge(layout.nodes.len())?;
    let (real_node_rects, label_rects) = collect_node_rect_entries(layout);
    work_budget.charge(layout.original_edges.len())?;
    for edge_index in 0..layout.original_edges.len() {
        let Some(rewrite) = rewrite_for_edge(
            layout,
            edge_index,
            &real_node_rects,
            &label_rects,
            work_budget,
        )?
        else {
            continue;
        };
        layout.original_edges[edge_index].points = rewrite.points;
        if let Some((label_node_id, anchor)) = rewrite.label_anchor
            && let Some(label) = layout.nodes.get_mut(&label_node_id)
        {
            label.x = anchor.x;
            label.y = anchor.y;
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

    fn candidate_points() -> Vec<LayoutPoint> {
        vec![
            point(0.0, 120.0),
            point(70.0, 120.0),
            point(70.0, 100.0),
            point(78.0, 100.0),
        ]
    }

    fn assert_points(actual: &[LayoutPoint], expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len());
        for (point, &(x, y)) in actual.iter().zip(expected) {
            assert!((point.x - x).abs() < EPSILON, "x: {} != {x}", point.x);
            assert!((point.y - y).abs() < EPSILON, "y: {} != {y}", point.y);
        }
    }

    #[test]
    fn retargets_short_perpendicular_stub_to_destination_face_center() {
        let mut layout = layout(
            vec![node(
                "B",
                WorkingNodeKind::Content,
                100.0,
                110.0,
                40.0,
                40.0,
            )],
            vec![edge("A_B", "A", "B", candidate_points())],
        );
        collapse_short_terminal_stub(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();
        assert_points(
            &layout.original_edges[0].points,
            &[(0.0, 120.0), (100.0, 120.0), (100.0, 130.0)],
        );
    }

    #[test]
    fn rejects_rewrite_when_extended_preceding_segment_hits_a_node() {
        let original = candidate_points();
        let mut layout = layout(
            vec![
                node("B", WorkingNodeKind::Content, 100.0, 110.0, 40.0, 40.0),
                node("blocker", WorkingNodeKind::Content, 50.0, 120.0, 10.0, 10.0),
            ],
            vec![edge("A_B", "A", "B", original.clone())],
        );
        collapse_short_terminal_stub(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();
        assert_points(
            &layout.original_edges[0].points,
            &original
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn rejects_rewrite_when_new_approach_strictly_crosses_another_edge() {
        let original = candidate_points();
        let mut layout = layout(
            vec![node(
                "B",
                WorkingNodeKind::Content,
                100.0,
                110.0,
                40.0,
                40.0,
            )],
            vec![
                edge("A_B", "A", "B", original.clone()),
                edge(
                    "C_D",
                    "C",
                    "D",
                    vec![point(90.0, 125.0), point(110.0, 125.0)],
                ),
            ],
        );
        collapse_short_terminal_stub(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();
        assert_points(
            &layout.original_edges[0].points,
            &original
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn reanchors_label_to_longest_fitting_segment_after_rewrite() {
        let mut labeled_edge = edge("A_B", "A", "B", candidate_points());
        labeled_edge.label_node_id = Some("label".to_string());
        let mut layout = layout(
            vec![
                node("B", WorkingNodeKind::Content, 100.0, 110.0, 40.0, 40.0),
                node(
                    "label",
                    WorkingNodeKind::EdgeLabel,
                    -100.0,
                    -100.0,
                    20.0,
                    8.0,
                ),
            ],
            vec![labeled_edge],
        );
        collapse_short_terminal_stub(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();
        let label = &layout.nodes["label"];
        assert!((label.x - 50.0).abs() < EPSILON);
        assert!((label.y - 120.0).abs() < EPSILON);
    }
}
