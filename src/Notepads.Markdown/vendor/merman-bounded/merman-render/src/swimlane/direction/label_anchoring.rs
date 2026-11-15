use super::super::working::{WorkingLayout, WorkingNodeKind};
use super::geometry::{
    EPSILON, RectBounds, dedupe_consecutive_points, rect_contains_rect, rect_of_node_bounds,
    rects_overlap, segment_bounds_overlap_rect,
};
use super::{LayoutWorkBudget, unordered_pair_count};
use crate::Result;
use crate::model::LayoutPoint;
use std::cmp::Ordering;

const MARKER_CLEARANCE_LENGTH: f64 = 10.0;
const MARKER_CLEARANCE_HALF_WIDTH: f64 = 7.0;
const LABEL_PLACEMENT_BUFFER: f64 = 3.0;
const LABEL_LANE_MARGIN: f64 = 1.0;
const LABEL_ENDPOINT_CLEARANCE: f64 = 12.0;
const ALONG_SEGMENT_PARAMETERS: [f64; 9] = [0.5, 0.25, 0.75, 0.05, 0.95, 0.15, 0.85, 0.1, 0.9];

#[derive(Debug, Clone)]
struct RectEntry {
    id: String,
    rect: RectBounds,
}

#[derive(Debug, Clone)]
struct EdgeSegment {
    edge_id: String,
    start: LayoutPoint,
    end: LayoutPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentOrientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy)]
struct SegmentCandidate {
    index: usize,
    length: f64,
    orientation: SegmentOrientation,
    midpoint: Anchor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
struct Placement {
    lane_id: String,
    anchor: Anchor,
    rect: RectBounds,
}

#[derive(Debug, Clone)]
struct Choice {
    lane_id: String,
    anchor: Anchor,
}

fn marker_clearance_rect_for(points: &[LayoutPoint], at_start: bool) -> Option<RectBounds> {
    if points.len() < 2 {
        return None;
    }

    let (tip, inner) = if at_start {
        (&points[0], &points[1])
    } else {
        (&points[points.len() - 1], &points[points.len() - 2])
    };
    let dx = inner.x - tip.x;
    let dy = inner.y - tip.y;
    let length = dx.abs() + dy.abs();
    if length < EPSILON {
        return None;
    }

    if dy.abs() <= EPSILON {
        let other_x = tip.x + dx.signum() * MARKER_CLEARANCE_LENGTH;
        return Some(RectBounds {
            left: tip.x.min(other_x),
            right: tip.x.max(other_x),
            top: tip.y - MARKER_CLEARANCE_HALF_WIDTH,
            bottom: tip.y + MARKER_CLEARANCE_HALF_WIDTH,
        });
    }

    if dx.abs() <= EPSILON {
        let other_y = tip.y + dy.signum() * MARKER_CLEARANCE_LENGTH;
        return Some(RectBounds {
            left: tip.x - MARKER_CLEARANCE_HALF_WIDTH,
            right: tip.x + MARKER_CLEARANCE_HALF_WIDTH,
            top: tip.y.min(other_y),
            bottom: tip.y.max(other_y),
        });
    }

    Some(RectBounds {
        left: tip.x.min(inner.x),
        right: tip.x.max(inner.x),
        top: tip.y.min(inner.y),
        bottom: tip.y.max(inner.y),
    })
}

fn normalize_rect(rect: RectBounds) -> RectBounds {
    RectBounds {
        left: rect.left.min(rect.right),
        right: rect.left.max(rect.right),
        top: rect.top.min(rect.bottom),
        bottom: rect.top.max(rect.bottom),
    }
}

fn point_inside_rect_inclusive(point: Anchor, rect: RectBounds) -> bool {
    point.x >= rect.left - EPSILON
        && point.x <= rect.right + EPSILON
        && point.y >= rect.top - EPSILON
        && point.y <= rect.bottom + EPSILON
}

fn label_overlaps_own_marker(rect: RectBounds, points: &[LayoutPoint]) -> bool {
    let visible_points = dedupe_consecutive_points(points, EPSILON);
    [
        marker_clearance_rect_for(&visible_points, true),
        marker_clearance_rect_for(&visible_points, false),
    ]
    .into_iter()
    .flatten()
    .any(|marker| rects_overlap(rect, normalize_rect(marker)))
}

fn collect_segments(points: &[LayoutPoint]) -> Vec<SegmentCandidate> {
    let mut segments = Vec::new();
    for (index, pair) in points.windows(2).enumerate() {
        let start = &pair[0];
        let end = &pair[1];
        let dx = (start.x - end.x).abs();
        let dy = (start.y - end.y).abs();
        if dx < EPSILON && dy < EPSILON {
            continue;
        }
        if dx >= EPSILON && dy >= EPSILON {
            continue;
        }
        segments.push(SegmentCandidate {
            index,
            length: dx + dy,
            orientation: if dx >= EPSILON {
                SegmentOrientation::Horizontal
            } else {
                SegmentOrientation::Vertical
            },
            midpoint: Anchor {
                x: (start.x + end.x) / 2.0,
                y: (start.y + end.y) / 2.0,
            },
        });
    }
    segments
}

fn rank_segments(
    pool: &[SegmentCandidate],
    label_long_axis: SegmentOrientation,
    label_width: f64,
    label_height: f64,
) -> Vec<SegmentCandidate> {
    let mut ranked = pool.to_vec();
    ranked.sort_by(|left, right| {
        let left_matches_long_axis = left.orientation == label_long_axis;
        let right_matches_long_axis = right.orientation == label_long_axis;
        if left_matches_long_axis != right_matches_long_axis {
            return right_matches_long_axis.cmp(&left_matches_long_axis);
        }

        let extent_for = |segment: &SegmentCandidate| match segment.orientation {
            SegmentOrientation::Horizontal => label_width,
            SegmentOrientation::Vertical => label_height,
        };
        let left_fits = left.length >= extent_for(left) + 2.0;
        let right_fits = right.length >= extent_for(right) + 2.0;
        if left_fits != right_fits {
            return right_fits.cmp(&left_fits);
        }

        right
            .length
            .partial_cmp(&left.length)
            .unwrap_or(Ordering::Equal)
    });
    ranked
}

struct AnchorSearch<'a> {
    lane_groups: &'a [RectEntry],
    foreign_node_rects: &'a [RectEntry],
    all_edge_segments: &'a [EdgeSegment],
    placed_label_rects: &'a [RectEntry],
    label_id: &'a str,
    edge_id: &'a str,
    points: &'a [LayoutPoint],
    segments: &'a [SegmentCandidate],
    label_width: f64,
    label_height: f64,
    label_long_axis: SegmentOrientation,
}

impl AnchorSearch<'_> {
    fn anchor_at_parameter(&self, segment: SegmentCandidate, parameter: f64) -> Anchor {
        let start = &self.points[segment.index];
        let end = &self.points[segment.index + 1];
        Anchor {
            x: start.x + (end.x - start.x) * parameter,
            y: start.y + (end.y - start.y) * parameter,
        }
    }

    fn find_containing_lane(
        &self,
        rect: RectBounds,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<&RectEntry>> {
        work_budget.charge(self.lane_groups.len())?;
        Ok(self
            .lane_groups
            .iter()
            .find(|lane| rect_contains_rect(lane.rect, rect)))
    }

    fn placement_for_anchor(
        &self,
        anchor: Anchor,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<Placement>> {
        let centered_rect =
            RectBounds::from_center(anchor.x, anchor.y, self.label_width, self.label_height);
        if let Some(lane) = self.find_containing_lane(centered_rect, work_budget)? {
            return Ok(Some(Placement {
                lane_id: lane.id.clone(),
                anchor,
                rect: centered_rect,
            }));
        }

        work_budget.charge(self.lane_groups.len())?;
        let Some(containing_lane) = self
            .lane_groups
            .iter()
            .find(|lane| point_inside_rect_inclusive(anchor, lane.rect))
        else {
            return Ok(None);
        };
        let minimum_x = containing_lane.rect.left + self.label_width / 2.0 + LABEL_LANE_MARGIN;
        let maximum_x = containing_lane.rect.right - self.label_width / 2.0 - LABEL_LANE_MARGIN;
        let minimum_y = containing_lane.rect.top + self.label_height / 2.0 + LABEL_LANE_MARGIN;
        let maximum_y = containing_lane.rect.bottom - self.label_height / 2.0 - LABEL_LANE_MARGIN;
        if minimum_x > maximum_x || minimum_y > maximum_y {
            return Ok(None);
        }

        let clamped_anchor = Anchor {
            x: anchor.x.clamp(minimum_x, maximum_x),
            y: anchor.y.clamp(minimum_y, maximum_y),
        };
        let clamped_rect = RectBounds::from_center(
            clamped_anchor.x,
            clamped_anchor.y,
            self.label_width,
            self.label_height,
        );
        Ok(
            point_inside_rect_inclusive(anchor, clamped_rect).then(|| Placement {
                lane_id: containing_lane.id.clone(),
                anchor: clamped_anchor,
                rect: clamped_rect,
            }),
        )
    }

    fn distance_along_segment(
        &self,
        segment: SegmentCandidate,
        anchor: Anchor,
        endpoint: &LayoutPoint,
    ) -> f64 {
        match segment.orientation {
            SegmentOrientation::Horizontal => (anchor.x - endpoint.x).abs(),
            SegmentOrientation::Vertical => (anchor.y - endpoint.y).abs(),
        }
    }

    fn label_clears_terminal_endpoints(&self, segment: SegmentCandidate, anchor: Anchor) -> bool {
        let half_extent = match segment.orientation {
            SegmentOrientation::Horizontal => self.label_width / 2.0,
            SegmentOrientation::Vertical => self.label_height / 2.0,
        };
        let required_distance = half_extent + LABEL_ENDPOINT_CLEARANCE;
        let first_visible_segment = self.segments[0];
        let last_visible_segment = self.segments[self.segments.len() - 1];

        if segment.index == first_visible_segment.index {
            let start = &self.points[segment.index];
            if self.distance_along_segment(segment, anchor, start) + EPSILON < required_distance {
                return false;
            }
        }
        if segment.index == last_visible_segment.index {
            let end = &self.points[segment.index + 1];
            if self.distance_along_segment(segment, anchor, end) + EPSILON < required_distance {
                return false;
            }
        }
        true
    }

    fn label_overlaps_foreign_node(
        &self,
        rect: RectBounds,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<bool> {
        let buffered = rect.inflated(LABEL_PLACEMENT_BUFFER);
        work_budget.charge(self.foreign_node_rects.len())?;
        Ok(self
            .foreign_node_rects
            .iter()
            .any(|node| node.id != self.label_id && rects_overlap(buffered, node.rect)))
    }

    fn label_overlaps_foreign_edge(
        &self,
        rect: RectBounds,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<bool> {
        let buffered = rect.inflated(LABEL_PLACEMENT_BUFFER);
        work_budget.charge(self.all_edge_segments.len())?;
        Ok(self.all_edge_segments.iter().any(|segment| {
            segment.edge_id != self.edge_id
                && segment_bounds_overlap_rect(&segment.start, &segment.end, buffered, 0.0)
        }))
    }

    fn overlaps_placed_label(
        &self,
        rect: RectBounds,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<bool> {
        work_budget.charge(self.placed_label_rects.len())?;
        Ok(self
            .placed_label_rects
            .iter()
            .any(|placed| placed.id != self.label_id && rects_overlap(rect, placed.rect)))
    }

    fn try_pool(
        &self,
        pool: &[SegmentCandidate],
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<Choice>> {
        work_budget.charge(pool.len().saturating_add(unordered_pair_count(pool.len())))?;
        let ranked = rank_segments(
            pool,
            self.label_long_axis,
            self.label_width,
            self.label_height,
        );
        for segment in ranked {
            for parameter in ALONG_SEGMENT_PARAMETERS {
                work_budget.charge(1)?;
                let anchor = self.anchor_at_parameter(segment, parameter);
                if !self.label_clears_terminal_endpoints(segment, anchor) {
                    continue;
                }
                let Some(placement) = self.placement_for_anchor(anchor, work_budget)? else {
                    continue;
                };
                work_budget.charge(self.points.len())?;
                if label_overlaps_own_marker(placement.rect, self.points) {
                    continue;
                }
                if self.overlaps_placed_label(placement.rect, work_budget)? {
                    continue;
                }
                if self.label_overlaps_foreign_node(placement.rect, work_budget)? {
                    continue;
                }
                if self.label_overlaps_foreign_edge(placement.rect, work_budget)? {
                    continue;
                }
                return Ok(Some(Choice {
                    lane_id: placement.lane_id,
                    anchor: placement.anchor,
                }));
            }
        }
        Ok(None)
    }

    fn find_lane_containing_fallback(
        &self,
        pool: &[SegmentCandidate],
        require_endpoint_clearance: bool,
        allow_foreign_edge_overlap: bool,
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<Choice>> {
        work_budget.charge(pool.len().saturating_add(unordered_pair_count(pool.len())))?;
        let ranked = rank_segments(
            pool,
            self.label_long_axis,
            self.label_width,
            self.label_height,
        );
        for segment in ranked {
            work_budget.charge(1)?;
            let anchor = segment.midpoint;
            if require_endpoint_clearance && !self.label_clears_terminal_endpoints(segment, anchor)
            {
                continue;
            }
            let Some(placement) = self.placement_for_anchor(anchor, work_budget)? else {
                continue;
            };
            work_budget.charge(self.points.len())?;
            if label_overlaps_own_marker(placement.rect, self.points) {
                continue;
            }
            if self.overlaps_placed_label(placement.rect, work_budget)? {
                continue;
            }
            if self.label_overlaps_foreign_node(placement.rect, work_budget)? {
                continue;
            }
            if !allow_foreign_edge_overlap
                && self.label_overlaps_foreign_edge(placement.rect, work_budget)?
            {
                continue;
            }
            return Ok(Some(Choice {
                lane_id: placement.lane_id,
                anchor: placement.anchor,
            }));
        }
        Ok(None)
    }

    fn find_choice(
        &self,
        middle_pool: &[SegmentCandidate],
        work_budget: &mut LayoutWorkBudget,
    ) -> Result<Option<Choice>> {
        if let Some(choice) = self.try_pool(middle_pool, work_budget)? {
            return Ok(Some(choice));
        }
        if middle_pool.len() < self.segments.len()
            && let Some(choice) = self.try_pool(self.segments, work_budget)?
        {
            return Ok(Some(choice));
        }
        for (require_endpoint_clearance, allow_foreign_edge_overlap) in
            [(true, false), (false, false), (false, true)]
        {
            if let Some(choice) = self.find_lane_containing_fallback(
                self.segments,
                require_endpoint_clearance,
                allow_foreign_edge_overlap,
                work_budget,
            )? {
                return Ok(Some(choice));
            }
        }
        Ok(None)
    }
}

pub(super) fn anchor_labels_to_polyline(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    // `original_edges` is the renderable edge set; layout-only split edges live in `graph_edges`.
    work_budget.charge(layout.original_edges.len())?;
    let edge_segment_work = layout.original_edges.iter().fold(0usize, |total, edge| {
        total.saturating_add(edge.points.len().saturating_sub(1))
    });
    work_budget.charge(edge_segment_work)?;
    let all_edge_segments = layout
        .original_edges
        .iter()
        .flat_map(|edge| {
            edge.points.windows(2).map(|pair| EdgeSegment {
                edge_id: edge.id.clone(),
                start: pair[0].clone(),
                end: pair[1].clone(),
            })
        })
        .collect::<Vec<_>>();

    let mut foreign_node_rects = Vec::new();
    let mut lane_groups = Vec::new();
    work_budget.charge(layout.nodes.len())?;
    for node in layout.nodes.values() {
        if node.kind == WorkingNodeKind::Group && node.parent_id.is_none() {
            if let Some(rect) = rect_of_node_bounds(node) {
                lane_groups.push(RectEntry {
                    id: node.id.clone(),
                    rect,
                });
            }
            continue;
        }
        if node.kind == WorkingNodeKind::Group || node.kind == WorkingNodeKind::EdgeLabel {
            continue;
        }
        if let Some(rect) = rect_of_node_bounds(node) {
            foreign_node_rects.push(RectEntry {
                id: node.id.clone(),
                rect,
            });
        }
    }

    let mut placed_label_rects = Vec::<RectEntry>::new();
    work_budget.charge(layout.original_edges.len())?;
    for edge in &layout.original_edges {
        let Some(label_id) = edge.label_node_id.as_deref() else {
            continue;
        };
        let Some(label_node) = layout.nodes.get(label_id) else {
            continue;
        };
        if edge.points.len() < 2 || label_node.width <= 0.0 || label_node.height <= 0.0 {
            continue;
        }
        let label_width = label_node.width;
        let label_height = label_node.height;
        work_budget.charge(edge.points.len().saturating_sub(1))?;
        let segments = collect_segments(&edge.points);
        if segments.is_empty() {
            continue;
        }

        let middle_segments = if segments.len() >= 3 {
            work_budget.charge(segments.len())?;
            segments
                .iter()
                .copied()
                .filter(|segment| segment.index > 0 && segment.index < segments.len() - 1)
                .collect::<Vec<_>>()
        } else {
            segments.clone()
        };
        let middle_pool = if middle_segments.is_empty() {
            &segments
        } else {
            &middle_segments
        };
        let search = AnchorSearch {
            lane_groups: &lane_groups,
            foreign_node_rects: &foreign_node_rects,
            all_edge_segments: &all_edge_segments,
            placed_label_rects: &placed_label_rects,
            label_id,
            edge_id: &edge.id,
            points: &edge.points,
            segments: &segments,
            label_width,
            label_height,
            label_long_axis: if label_width >= label_height {
                SegmentOrientation::Horizontal
            } else {
                SegmentOrientation::Vertical
            },
        };
        let Some(choice) = search.find_choice(middle_pool, work_budget)? else {
            continue;
        };

        work_budget.charge(placed_label_rects.len())?;
        if let Some(label_node) = layout.nodes.get_mut(label_id) {
            label_node.x = choice.anchor.x;
            label_node.y = choice.anchor.y;
            label_node.parent_id = Some(choice.lane_id.clone());
            label_node.top_lane_id = Some(choice.lane_id);
        }
        let chosen_rect =
            RectBounds::from_center(choice.anchor.x, choice.anchor.y, label_width, label_height);
        if let Some(prior) = placed_label_rects
            .iter_mut()
            .find(|placed| placed.id == label_id)
        {
            prior.rect = chosen_rect;
        } else {
            placed_label_rects.push(RectEntry {
                id: label_id.to_string(),
                rect: chosen_rect,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::super::working::{WorkingEdge, WorkingNode};
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

    fn edge(id: &str, points: Vec<LayoutPoint>, label_node_id: Option<&str>) -> WorkingEdge {
        WorkingEdge {
            id: id.to_string(),
            from: format!("{id}-source"),
            to: format!("{id}-target"),
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

    fn assert_position(layout: &WorkingLayout, id: &str, x: f64, y: f64) {
        let actual = &layout.nodes[id];
        assert!((actual.x - x).abs() < EPSILON, "{} != {x}", actual.x);
        assert!((actual.y - y).abs() < EPSILON, "{} != {y}", actual.y);
    }

    #[test]
    fn anchors_to_the_only_middle_segment_and_reassigns_the_lane() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 100.0, 100.0, 200.0, 200.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 30.0, 10.0),
            ],
            vec![edge(
                "edge",
                vec![
                    point(20.0, 10.0),
                    point(20.0, 50.0),
                    point(150.0, 50.0),
                    point(150.0, 90.0),
                ],
                Some("label"),
            )],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", 85.0, 50.0);
        assert_eq!(layout.nodes["label"].parent_id.as_deref(), Some("lane"));
        assert_eq!(layout.nodes["label"].top_lane_id.as_deref(), Some("lane"));
    }

    #[test]
    fn anchor_candidate_is_rejected_before_scanning_all_edge_segments() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 100.0, 100.0, 200.0, 200.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 30.0, 10.0),
            ],
            vec![edge(
                "edge",
                vec![
                    point(20.0, 10.0),
                    point(20.0, 50.0),
                    point(150.0, 50.0),
                    point(150.0, 90.0),
                ],
                Some("label"),
            )],
        );
        let policy = crate::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 20)
            .unwrap();
        let mut budget = LayoutWorkBudget::new(policy, 0).unwrap();

        let error = anchor_labels_to_polyline(&mut layout, &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 23);
        assert_eq!(error.max, 20);
        assert_position(&layout, "label", 0.0, 0.0);
        assert_eq!(layout.nodes["label"].parent_id, None);
        assert_eq!(layout.nodes["label"].top_lane_id, None);
    }

    #[test]
    fn shifts_along_a_segment_to_avoid_a_foreign_edge() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 100.0, 100.0, 200.0, 200.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 20.0, 10.0),
            ],
            vec![
                edge(
                    "labelled",
                    vec![
                        point(20.0, 30.0),
                        point(20.0, 100.0),
                        point(180.0, 100.0),
                        point(180.0, 170.0),
                    ],
                    Some("label"),
                ),
                edge(
                    "foreign",
                    vec![point(100.0, 80.0), point(100.0, 120.0)],
                    None,
                ),
            ],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", 60.0, 100.0);
    }

    #[test]
    fn clamps_a_label_inside_its_lane_while_retaining_the_polyline_point() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 100.0, 50.0, 200.0, 100.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 20.0, 20.0),
            ],
            vec![edge(
                "edge",
                vec![
                    point(20.0, 30.0),
                    point(20.0, 3.0),
                    point(180.0, 3.0),
                    point(180.0, 30.0),
                ],
                Some("label"),
            )],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", 100.0, 11.0);
    }

    #[test]
    fn expands_from_cross_lane_middle_segments_to_all_segments() {
        let mut layout = layout(
            vec![
                node("top", WorkingNodeKind::Group, 100.0, 50.0, 200.0, 100.0),
                node("bottom", WorkingNodeKind::Group, 100.0, 150.0, 200.0, 100.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 20.0, 80.0),
                node(
                    "middle-blocker",
                    WorkingNodeKind::Content,
                    80.0,
                    100.0,
                    30.0,
                    100.0,
                ),
            ],
            vec![edge(
                "edge",
                vec![
                    point(20.0, 50.0),
                    point(80.0, 50.0),
                    point(80.0, 150.0),
                    point(140.0, 150.0),
                ],
                Some("label"),
            )],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", 50.0, 50.0);
        assert_eq!(layout.nodes["label"].parent_id.as_deref(), Some("top"));
    }

    #[test]
    fn prefers_a_segment_matching_the_label_long_axis() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 50.0, 50.0, 200.0, 200.0),
                node("label", WorkingNodeKind::EdgeLabel, 0.0, 0.0, 10.0, 30.0),
            ],
            vec![edge(
                "edge",
                vec![point(0.0, 0.0), point(100.0, 0.0), point(100.0, 100.0)],
                Some("label"),
            )],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", 100.0, 50.0);
    }

    #[test]
    fn rejects_a_terminal_label_that_overlaps_both_markers() {
        let mut layout = layout(
            vec![
                node("lane", WorkingNodeKind::Group, 20.0, 0.0, 100.0, 100.0),
                node(
                    "label",
                    WorkingNodeKind::EdgeLabel,
                    -20.0,
                    -20.0,
                    30.0,
                    10.0,
                ),
            ],
            vec![edge(
                "edge",
                vec![point(0.0, 0.0), point(40.0, 0.0)],
                Some("label"),
            )],
        );

        anchor_labels_to_polyline(&mut layout, &mut LayoutWorkBudget::unbounded_for_tests())
            .unwrap();

        assert_position(&layout, "label", -20.0, -20.0);
        assert_eq!(layout.nodes["label"].parent_id, None);
    }
}
