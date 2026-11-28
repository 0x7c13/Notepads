use super::bounds::recompute_nested_group_bounds;
use super::config::LR_TITLE_BAND_SIZE;
use super::geometry::Rect;
pub(super) use super::work_budget::{LayoutWorkBudget, unordered_pair_count};
use super::working::{WorkingLayout, WorkingNodeKind};
use crate::Result;
use crate::model::{LayoutPoint, SwimlaneDirection, SwimlaneTitleRect};
use std::collections::HashMap;

mod detour_simplification;
mod endpoint_clip;
pub(super) mod geometry;
mod label_anchoring;
mod materialized_geometry;
mod port_swap;
mod shared_track_nudging;
mod sibling_shared_face_routing;
mod terminal_stub;
#[cfg(test)]
mod validation;

use geometry::{orthogonalize_polyline, simplify_polyline};

fn layout_content_ids(layout: &WorkingLayout) -> Vec<String> {
    layout
        .nodes
        .values()
        .filter(|node| !node.is_group() && node.kind != WorkingNodeKind::Dummy)
        .map(|node| node.id.clone())
        .collect()
}

fn mirror_axis(layout: &mut WorkingLayout, horizontal: bool) {
    let content_ids = layout_content_ids(layout);
    let values: Vec<f64> = content_ids
        .iter()
        .filter_map(|id| layout.nodes.get(id))
        .map(|node| if horizontal { node.x } else { node.y })
        .collect();
    let Some(minimum) = values.iter().copied().reduce(f64::min) else {
        return;
    };
    let Some(maximum) = values.iter().copied().reduce(f64::max) else {
        return;
    };
    let mirror = |value: f64| minimum + maximum - value;
    for node in layout
        .nodes
        .values_mut()
        .filter(|node| node.kind != WorkingNodeKind::Dummy)
    {
        if horizontal {
            node.x = mirror(node.x);
            if let Some(rect) = &mut node.title_rect {
                let left = mirror(rect.right);
                let right = mirror(rect.left);
                rect.left = left;
                rect.right = right;
            }
        } else {
            node.y = mirror(node.y);
            if let Some(rect) = &mut node.title_rect {
                let top = mirror(rect.bottom);
                let bottom = mirror(rect.top);
                rect.top = top;
                rect.bottom = bottom;
            }
        }
    }
    for edge in &mut layout.original_edges {
        for point in &mut edge.points {
            if horizontal {
                point.x = mirror(point.x);
            } else {
                point.y = mirror(point.y);
            }
        }
    }
}

fn transform_lr(layout: &mut WorkingLayout) {
    let content_ids = layout_content_ids(layout);
    if content_ids.is_empty() {
        return;
    }
    let min_x = content_ids
        .iter()
        .map(|id| layout.nodes[id].x)
        .fold(f64::INFINITY, f64::min);
    let min_y = content_ids
        .iter()
        .map(|id| layout.nodes[id].y)
        .fold(f64::INFINITY, f64::min);
    let (total_width, total_height) = content_ids.iter().fold((0.0, 0.0), |acc, id| {
        let node = &layout.nodes[id];
        (acc.0 + node.width, acc.1 + node.height)
    });
    let average_width = total_width / content_ids.len() as f64;
    let average_height = total_height / content_ids.len() as f64;
    let horizontal_scale = if average_height > 0.0 {
        (average_width / average_height).max(1.0)
    } else {
        1.0
    };
    let transform = |point: &LayoutPoint| LayoutPoint {
        x: (point.y - min_y) * horizontal_scale + LR_TITLE_BAND_SIZE,
        y: point.x - min_x,
    };

    for id in &content_ids {
        if let Some(node) = layout.nodes.get_mut(id) {
            let transformed = transform(&LayoutPoint {
                x: node.x,
                y: node.y,
            });
            node.x = transformed.x;
            node.y = transformed.y;
        }
    }
    for edge in &mut layout.original_edges {
        for point in &mut edge.points {
            *point = transform(point);
        }
    }

    recompute_nested_group_bounds(layout);
    let top_lane_ids: Vec<String> = layout
        .nodes
        .values()
        .filter(|node| node.is_group() && node.parent_id.is_none())
        .map(|node| node.id.clone())
        .collect();
    let mut children_by_lane: HashMap<String, Vec<String>> = HashMap::new();
    for id in &content_ids {
        let Some(lane_id) = layout.nodes[id].top_lane_id.clone() else {
            continue;
        };
        children_by_lane
            .entry(lane_id)
            .or_default()
            .push(id.clone());
    }

    let max_padding = top_lane_ids
        .iter()
        .map(|id| layout.nodes[id].padding)
        .fold(0.0, f64::max);
    let mut lane_bounds = Vec::new();
    let mut global_min_x = f64::INFINITY;
    let mut global_max_x = f64::NEG_INFINITY;
    for id in &top_lane_ids {
        let children = children_by_lane.get(id).cloned().unwrap_or_default();
        let mut bounds: Option<Rect> = None;
        for child_id in children {
            let child = &layout.nodes[&child_id];
            let rect = Rect::from_center(child.x, child.y, child.width, child.height);
            if let Some(current) = &mut bounds {
                current.union(rect);
            } else {
                bounds = Some(rect);
            }
        }
        let Some(bounds) = bounds else {
            continue;
        };
        global_min_x = global_min_x.min(bounds.left);
        global_max_x = global_max_x.max(bounds.right);
        lane_bounds.push((
            id.clone(),
            bounds.top,
            bounds.bottom,
            (bounds.top + bounds.bottom) / 2.0,
        ));
    }
    if !global_min_x.is_finite() || !global_max_x.is_finite() {
        return;
    }

    let horizontal_margin = max_padding.max(10.0);
    let body_width = global_max_x - global_min_x + 2.0 * horizontal_margin;
    let lane_width = LR_TITLE_BAND_SIZE + body_width;
    let body_center = (global_min_x + global_max_x) / 2.0;
    let body_left = body_center - body_width / 2.0;
    let lane_left = body_left - LR_TITLE_BAND_SIZE;
    let center_x = lane_left + lane_width / 2.0;
    let vertical_margin = max_padding.max(LR_TITLE_BAND_SIZE);
    lane_bounds.sort_by(|left, right| left.3.total_cmp(&right.3));

    for index in 0..lane_bounds.len() {
        let (id, content_top, content_bottom, _) = &lane_bounds[index];
        let top = if index == 0 {
            content_top - vertical_margin
        } else {
            (lane_bounds[index - 1].2 + content_top) / 2.0
        };
        let bottom = if index + 1 == lane_bounds.len() {
            content_bottom + vertical_margin
        } else {
            (content_bottom + lane_bounds[index + 1].1) / 2.0
        };
        if let Some(lane) = layout.nodes.get_mut(id) {
            lane.x = center_x;
            lane.y = (top + bottom) / 2.0;
            lane.width = lane_width;
            lane.height = (bottom - top).max(0.0);
            lane.content_top = Some(*content_top);
            lane.title_rect = Some(SwimlaneTitleRect {
                left: lane_left,
                right: lane_left + LR_TITLE_BAND_SIZE,
                top,
                bottom,
            });
        }
    }
}

fn finalize_rendered_edges(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    materialized_geometry::resolve_rendered_orthogonal_crossings(layout, work_budget)?;
    materialized_geometry::reassign_crossing_external_rail_channels(layout, work_budget)?;
    materialized_geometry::shortcut_redundant_orthogonal_jogs(layout, work_budget)?;
    label_anchoring::anchor_labels_to_polyline(layout, work_budget)?;
    endpoint_clip::prepare_edge_endpoints_for_renderer(layout);
    materialized_geometry::lift_obstacle_hugging_same_side_rails(layout, work_budget)?;
    label_anchoring::anchor_labels_to_polyline(layout, work_budget)?;
    endpoint_clip::prepare_edge_endpoints_for_renderer(layout);
    Ok(())
}

fn post_process_staged(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    match layout.direction {
        SwimlaneDirection::Tb => {}
        SwimlaneDirection::Bt => mirror_axis(layout, false),
        SwimlaneDirection::Lr => transform_lr(layout),
        SwimlaneDirection::Rl => {
            transform_lr(layout);
            mirror_axis(layout, true);
        }
    }
    for edge in &mut layout.original_edges {
        edge.points = simplify_polyline(&orthogonalize_polyline(&edge.points));
    }
    detour_simplification::simplify_detoured_edges(layout, work_budget)?;
    sibling_shared_face_routing::straighten_collinear_sibling_detours(layout, work_budget)?;
    port_swap::port_swap_to_l_shape(layout, work_budget)?;
    label_anchoring::anchor_labels_to_polyline(layout, work_budget)?;
    endpoint_clip::clip_edge_endpoints_to_node_boundaries(layout);
    terminal_stub::collapse_short_terminal_stub(layout, work_budget)?;
    shared_track_nudging::nudge_shared_interior_subpaths(layout, work_budget)?;
    materialized_geometry::separate_shared_rendered_terminal_lanes(layout, work_budget)?;
    materialized_geometry::collapse_redundant_rectangular_doglegs(layout, work_budget)?;
    materialized_geometry::lift_obstacle_hugging_same_side_rails(layout, work_budget)?;
    materialized_geometry::swap_destination_terminal_tails_to_reduce_crossings(
        layout,
        work_budget,
    )?;

    finalize_rendered_edges(layout, work_budget)?;
    shared_track_nudging::nudge_shared_interior_subpaths(layout, work_budget)?;
    finalize_rendered_edges(layout, work_budget)?;

    for _ in 0..2 {
        materialized_geometry::lift_top_lane_title_bands_above_rails(layout, work_budget)?;
        materialized_geometry::shift_left_lane_title_bands_left_of_rails(layout, work_budget)?;
    }
    Ok(())
}

fn post_process_snapshot_ticks(layout: &WorkingLayout) -> usize {
    let edge_points = layout
        .graph_edges
        .iter()
        .chain(&layout.original_edges)
        .map(|edge| edge.points.len())
        .fold(0usize, usize::saturating_add);
    layout
        .nodes
        .len()
        .saturating_add(layout.graph_edges.len())
        .saturating_add(layout.original_edges.len())
        .saturating_add(layout.top_lane_order.len())
        .saturating_add(edge_points)
}

pub(super) fn post_process(
    layout: &mut WorkingLayout,
    work_budget: &mut LayoutWorkBudget,
) -> Result<()> {
    // Reserve the snapshot and initial transform cost before cloning any geometry.
    work_budget.charge(post_process_snapshot_ticks(layout))?;
    work_budget.finish()?;

    // One transaction-wide snapshot keeps resource failures atomic without
    // cloning the complete layout again inside each finalization pass.
    let mut staged = layout.clone();
    post_process_staged(&mut staged, work_budget)?;
    work_budget.finish()?;
    *layout = staged;
    Ok(())
}

#[cfg(test)]
mod resource_tests {
    use super::{LayoutWorkBudget, finalize_rendered_edges, post_process};
    use crate::model::{LayoutPoint, SwimlaneDirection};
    use crate::resources::{RenderResourcePolicy, ResourceLimitId};
    use crate::swimlane::working::{WorkingEdge, WorkingLayout, WorkingNode, WorkingNodeKind};
    use indexmap::IndexMap;

    fn policy(max: usize) -> RenderResourcePolicy {
        RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, max)
            .unwrap()
    }

    #[test]
    fn repeated_finalizers_share_one_cumulative_operation_budget() {
        let nodes = [
            ("A", -20.0, 0.0),
            ("B", 20.0, 0.0),
            ("C", -20.0, 20.0),
            ("D", 20.0, 20.0),
        ]
        .into_iter()
        .map(|(id, x, y)| {
            (
                id.to_string(),
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
                    width: 10.0,
                    height: 10.0,
                    label_width: 10.0,
                    label_height: 10.0,
                    layer: 0,
                    order: 0,
                    content_top: None,
                    title_rect: None,
                },
            )
        })
        .collect::<IndexMap<_, _>>();
        let edges = [("A_B", "A", "B", 0.0), ("C_D", "C", "D", 20.0)]
            .into_iter()
            .map(|(id, from, to, y)| WorkingEdge {
                id: id.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                reference_id: id.to_string(),
                label_node_id: None,
                reversed_for_layout: false,
                points: vec![LayoutPoint { x: -15.0, y }, LayoutPoint { x: 15.0, y }],
            })
            .collect();
        let mut layout = WorkingLayout {
            direction: SwimlaneDirection::Tb,
            nodes,
            graph_edges: Vec::new(),
            original_edges: edges,
            top_lane_order: Vec::new(),
        };
        let mut budget = LayoutWorkBudget::new(policy(60), 0).unwrap();

        finalize_rendered_edges(&mut layout, &mut budget).unwrap();
        assert_eq!(budget.used(), 60);
        let before_second = layout
            .original_edges
            .iter()
            .map(|edge| {
                edge.points
                    .iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let error = finalize_rendered_edges(&mut layout, &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(error.actual, 64);
        assert_eq!(error.max, 60);
        assert_eq!(budget.used(), 60);
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
            before_second
        );
    }

    #[test]
    fn post_process_budget_error_does_not_commit_staged_geometry() {
        let nodes = [("A", 0.0), ("B", 20.0)]
            .into_iter()
            .map(|(id, y)| {
                (
                    id.to_string(),
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
                        x: 0.0,
                        y,
                        width: 10.0,
                        height: 10.0,
                        label_width: 10.0,
                        label_height: 10.0,
                        layer: 0,
                        order: 0,
                        content_top: None,
                        title_rect: None,
                    },
                )
            })
            .collect::<IndexMap<_, _>>();
        let mut layout = WorkingLayout {
            direction: SwimlaneDirection::Bt,
            nodes,
            graph_edges: Vec::new(),
            original_edges: vec![WorkingEdge {
                id: "A_B".to_string(),
                from: "A".to_string(),
                to: "B".to_string(),
                reference_id: "A_B".to_string(),
                label_node_id: None,
                reversed_for_layout: false,
                points: vec![
                    LayoutPoint { x: 0.0, y: 5.0 },
                    LayoutPoint { x: 0.0, y: 15.0 },
                ],
            }],
            top_lane_order: Vec::new(),
        };
        let before_nodes = layout
            .nodes
            .values()
            .map(|node| (node.id.clone(), node.x, node.y))
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
        // Model and routing work may consume the operation budget before post-processing starts.
        // Start at the legal positive boundary so snapshot cost is rejected before cloning.
        let mut budget = LayoutWorkBudget::new(policy(1), 1).unwrap();

        let error = post_process(&mut layout, &mut budget).unwrap_err();
        let crate::Error::ResourceLimitExceeded(error) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(budget.used(), 1);
        assert_eq!(error.actual, 6);
        assert_eq!(error.max, 1);
        assert_eq!(
            layout
                .nodes
                .values()
                .map(|node| (node.id.clone(), node.x, node.y))
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
}
