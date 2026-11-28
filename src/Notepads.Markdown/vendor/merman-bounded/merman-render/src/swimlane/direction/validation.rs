use super::super::working::WorkingLayout;
use super::geometry::{RectBounds, collect_layout_node_rects, segment_bounds_overlap_rect};
use crate::model::LayoutPoint;
use std::collections::HashSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ValidationIssueKind {
    EdgeNodeOverlap,
    EdgeEdgeCrossing,
}

impl fmt::Display for ValidationIssueKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EdgeNodeOverlap => "edge-node-overlap",
            Self::EdgeEdgeCrossing => "edge-edge-crossing",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ValidationIssue {
    pub kind: ValidationIssueKind,
    pub edge_id: String,
    pub target_id: String,
    pub detail: String,
}

#[derive(Debug)]
struct EdgeSegment<'a> {
    edge_id: &'a str,
    start: &'a str,
    end: &'a str,
    first: &'a LayoutPoint,
    second: &'a LayoutPoint,
}

fn segments_intersect(
    first_start: &LayoutPoint,
    first_end: &LayoutPoint,
    second_start: &LayoutPoint,
    second_end: &LayoutPoint,
) -> bool {
    let first_dx = first_end.x - first_start.x;
    let first_dy = first_end.y - first_start.y;
    let second_dx = second_end.x - second_start.x;
    let second_dy = second_end.y - second_start.y;
    let cross = first_dx * second_dy - first_dy * second_dx;
    if cross.abs() < 1e-10 {
        return false;
    }

    let dx = second_start.x - first_start.x;
    let dy = second_start.y - first_start.y;
    let first_parameter = (dx * second_dy - dy * second_dx) / cross;
    let second_parameter = (dx * first_dy - dy * first_dx) / cross;
    const ENDPOINT_EPSILON: f64 = 0.01;
    first_parameter > ENDPOINT_EPSILON
        && first_parameter < 1.0 - ENDPOINT_EPSILON
        && second_parameter > ENDPOINT_EPSILON
        && second_parameter < 1.0 - ENDPOINT_EPSILON
}

pub(super) fn validate_swimlanes_layout(layout: &WorkingLayout) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if layout.original_edges.is_empty() || layout.nodes.is_empty() {
        return issues;
    }

    let node_rects = collect_layout_node_rects(layout, true);
    let mut edge_segments = Vec::new();
    const NODE_OVERLAP_EPSILON: f64 = 1.0;

    for edge in &layout.original_edges {
        if edge.points.len() < 2 {
            continue;
        }

        for node in &node_rects {
            if node.node_id == edge.from
                || node.node_id == edge.to
                || edge
                    .label_node_id
                    .as_deref()
                    .is_some_and(|label_id| node.node_id == label_id)
            {
                continue;
            }
            let rect = RectBounds {
                left: node.left,
                right: node.right,
                top: node.top,
                bottom: node.bottom,
            };
            for (segment_index, pair) in edge.points.windows(2).enumerate() {
                if segment_bounds_overlap_rect(&pair[0], &pair[1], rect, -NODE_OVERLAP_EPSILON) {
                    issues.push(ValidationIssue {
                        kind: ValidationIssueKind::EdgeNodeOverlap,
                        edge_id: edge.id.clone(),
                        target_id: node.node_id.clone(),
                        detail: format!(
                            "segment {segment_index} passes through node \"{}\"",
                            node.node_id
                        ),
                    });
                    break;
                }
            }
        }

        edge_segments.extend(edge.points.windows(2).map(|pair| EdgeSegment {
            edge_id: &edge.id,
            start: &edge.from,
            end: &edge.to,
            first: &pair[0],
            second: &pair[1],
        }));
    }

    let mut crossing_pairs = HashSet::new();
    for (first_index, first) in edge_segments.iter().enumerate() {
        for second in &edge_segments[first_index + 1..] {
            if first.edge_id == second.edge_id {
                continue;
            }
            if first.start == second.start
                || first.start == second.end
                || first.end == second.start
                || first.end == second.end
            {
                continue;
            }
            if !segments_intersect(first.first, first.second, second.first, second.second) {
                continue;
            }

            let pair_key = if first.edge_id < second.edge_id {
                format!("{}|{}", first.edge_id, second.edge_id)
            } else {
                format!("{}|{}", second.edge_id, first.edge_id)
            };
            if crossing_pairs.insert(pair_key) {
                issues.push(ValidationIssue {
                    kind: ValidationIssueKind::EdgeEdgeCrossing,
                    edge_id: first.edge_id.to_string(),
                    target_id: second.edge_id.to_string(),
                    detail: format!(
                        "edges \"{}\" and \"{}\" cross",
                        first.edge_id, second.edge_id
                    ),
                });
            }
        }
    }

    issues
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

    #[test]
    fn preserves_upstream_issue_type_names() {
        assert_eq!(
            ValidationIssueKind::EdgeNodeOverlap.to_string(),
            "edge-node-overlap"
        );
        assert_eq!(
            ValidationIssueKind::EdgeEdgeCrossing.to_string(),
            "edge-edge-crossing"
        );
    }

    #[test]
    fn reports_an_edge_segment_passing_through_a_non_endpoint_node() {
        let layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 0.0, 0.0, 20.0, 20.0),
                node("B", WorkingNodeKind::Content, 100.0, 0.0, 20.0, 20.0),
                node("C", WorkingNodeKind::Content, 50.0, 0.0, 20.0, 20.0),
            ],
            vec![edge(
                "A_B",
                "A",
                "B",
                vec![point(10.0, 0.0), point(90.0, 0.0)],
                None,
            )],
        );

        assert_eq!(
            validate_swimlanes_layout(&layout),
            vec![ValidationIssue {
                kind: ValidationIssueKind::EdgeNodeOverlap,
                edge_id: "A_B".to_string(),
                target_id: "C".to_string(),
                detail: "segment 0 passes through node \"C\"".to_string(),
            }]
        );
    }

    #[test]
    fn excludes_an_edges_own_label_but_reports_a_foreign_edge_through_it() {
        let layout = layout(
            vec![
                node("A", WorkingNodeKind::Content, 0.0, 0.0, 20.0, 20.0),
                node("B", WorkingNodeKind::Content, 100.0, 0.0, 20.0, 20.0),
                node("label", WorkingNodeKind::EdgeLabel, 50.0, 0.0, 20.0, 10.0),
            ],
            vec![
                edge(
                    "own",
                    "A",
                    "B",
                    vec![point(10.0, 0.0), point(90.0, 0.0)],
                    Some("label"),
                ),
                edge(
                    "foreign",
                    "",
                    "",
                    vec![point(50.0, -20.0), point(50.0, 20.0)],
                    None,
                ),
            ],
        );

        let issues = validate_swimlanes_layout(&layout);

        assert!(!issues.iter().any(|issue| {
            issue.edge_id == "own"
                && issue.target_id == "label"
                && issue.kind == ValidationIssueKind::EdgeNodeOverlap
        }));
        assert!(issues.iter().any(|issue| {
            issue.edge_id == "foreign"
                && issue.target_id == "label"
                && issue.kind == ValidationIssueKind::EdgeNodeOverlap
        }));
    }

    #[test]
    fn reports_one_issue_for_a_proper_crossing_between_distinct_edges() {
        let layout = layout(
            vec![node("lane", WorkingNodeKind::Group, 0.0, 0.0, 200.0, 200.0)],
            vec![
                edge(
                    "horizontal",
                    "A",
                    "B",
                    vec![point(-50.0, 0.0), point(50.0, 0.0)],
                    None,
                ),
                edge(
                    "vertical",
                    "C",
                    "D",
                    vec![point(0.0, -50.0), point(0.0, 50.0)],
                    None,
                ),
            ],
        );

        assert_eq!(
            validate_swimlanes_layout(&layout),
            vec![ValidationIssue {
                kind: ValidationIssueKind::EdgeEdgeCrossing,
                edge_id: "horizontal".to_string(),
                target_id: "vertical".to_string(),
                detail: "edges \"horizontal\" and \"vertical\" cross".to_string(),
            }]
        );
    }

    #[test]
    fn ignores_crossings_between_edges_that_share_any_endpoint() {
        let layout = layout(
            vec![node("lane", WorkingNodeKind::Group, 0.0, 0.0, 200.0, 200.0)],
            vec![
                edge(
                    "first",
                    "A",
                    "B",
                    vec![point(-50.0, 0.0), point(50.0, 0.0)],
                    None,
                ),
                edge(
                    "second",
                    "A",
                    "C",
                    vec![point(0.0, -50.0), point(0.0, 50.0)],
                    None,
                ),
            ],
        );

        assert!(validate_swimlanes_layout(&layout).is_empty());
    }

    #[test]
    fn ignores_collinear_segments_and_endpoint_touches() {
        let layout = layout(
            vec![node("lane", WorkingNodeKind::Group, 0.0, 0.0, 200.0, 200.0)],
            vec![
                edge(
                    "first",
                    "A",
                    "B",
                    vec![point(-50.0, 0.0), point(0.0, 0.0)],
                    None,
                ),
                edge(
                    "collinear",
                    "C",
                    "D",
                    vec![point(-25.0, 0.0), point(25.0, 0.0)],
                    None,
                ),
                edge(
                    "touching",
                    "E",
                    "F",
                    vec![point(0.0, 0.0), point(0.0, 50.0)],
                    None,
                ),
            ],
        );

        assert!(validate_swimlanes_layout(&layout).is_empty());
    }

    #[test]
    fn reports_each_crossing_edge_pair_only_once() {
        let layout = layout(
            vec![node("lane", WorkingNodeKind::Group, 0.0, 0.0, 300.0, 300.0)],
            vec![
                edge(
                    "horizontal",
                    "A",
                    "B",
                    vec![
                        point(-100.0, -25.0),
                        point(100.0, -25.0),
                        point(100.0, 25.0),
                        point(-100.0, 25.0),
                    ],
                    None,
                ),
                edge(
                    "vertical",
                    "C",
                    "D",
                    vec![point(0.0, -100.0), point(0.0, 100.0)],
                    None,
                ),
            ],
        );

        let issues = validate_swimlanes_layout(&layout);

        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.kind == ValidationIssueKind::EdgeEdgeCrossing)
                .count(),
            1
        );
    }
}
