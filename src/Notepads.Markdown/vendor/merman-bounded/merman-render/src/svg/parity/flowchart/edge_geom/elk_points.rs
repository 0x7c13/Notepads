//! ELK edge point post-processing for flowchart SVG parity.
//!
//! Source port boundary:
//! - Mermaid `packages/mermaid/src/rendering-util/layout-algorithms/elk/render.ts` center-point injection, `cutter2`,
//!   endpoint replacement, invalid-point fallback, consecutive deduplication, and rounded curve
//!   selection.
//! - Mermaid `packages/mermaid/src/rendering-util/layout-algorithms/elk/geometry.ts` `outsideNode` / `replaceEndpoint`
//!   endpoint semantics.

use super::super::*;
use super::{BoundaryNode, boundary_for_node, intersect_for_layout_shape};

pub(in crate::svg::parity::flowchart) fn missing_section_points(
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
) -> Option<Vec<crate::model::LayoutPoint>> {
    let start = boundary_for_node(ctx, &edge.from, origin_x, origin_y)?;
    let end = boundary_for_node(ctx, &edge.to, origin_x, origin_y)?;
    let centers = [
        crate::model::LayoutPoint {
            x: start.x,
            y: start.y,
        },
        crate::model::LayoutPoint { x: end.x, y: end.y },
    ];
    let mut clipped = Vec::new();
    sanitize_flowchart_elk_points(ctx, edge, origin_x, origin_y, &centers, false, &mut clipped);
    Some(clipped)
}

pub(in crate::svg::parity::flowchart) fn missing_section_label_position(
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::model::LayoutEdge,
    origin_x: f64,
    origin_y: f64,
) -> Option<crate::model::LayoutPoint> {
    if !ctx.uses_elk_adapter_dom || !edge.points.is_empty() {
        return None;
    }
    let source = ctx.edges_by_id.get(edge.id.as_str())?;
    let points = missing_section_points(ctx, source, origin_x, origin_y)?;
    let first = points.first()?;
    let last = points.last()?;
    Some(crate::model::LayoutPoint {
        x: (first.x + last.x) / 2.0,
        y: (first.y + last.y) / 2.0,
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub(in crate::svg::parity::flowchart) struct ElkEndpointAdapterCorners {
    pub source: bool,
    pub target: bool,
}

pub(in crate::svg::parity::flowchart) fn apply_flowchart_elk_endpoint_cutter(
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
    base_points: &[crate::model::LayoutPoint],
    out: &mut Vec<crate::model::LayoutPoint>,
) -> ElkEndpointAdapterCorners {
    sanitize_flowchart_elk_points(ctx, edge, origin_x, origin_y, base_points, true, out)
}

// Adapt family-owned outlines to the shared source port; no provider points are mutated.
#[allow(clippy::too_many_arguments)]
fn sanitize_flowchart_elk_points(
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
    base_points: &[crate::model::LayoutPoint],
    has_section: bool,
    out: &mut Vec<crate::model::LayoutPoint>,
) -> ElkEndpointAdapterCorners {
    use crate::elk_edge_geometry::{self as geometry, Outline, Shape};
    out.clear();
    out.extend_from_slice(base_points);
    let Some(start_bounds) = boundary_for_node(ctx, &edge.from, origin_x, origin_y) else {
        return ElkEndpointAdapterCorners::default();
    };
    let Some(end_bounds) = boundary_for_node(ctx, &edge.to, origin_x, origin_y) else {
        return ElkEndpointAdapterCorners::default();
    };
    let layout_node = |id: &str, bounds: &BoundaryNode| crate::model::LayoutNode {
        id: id.to_owned(),
        x: bounds.x,
        y: bounds.y,
        width: bounds.width,
        height: bounds.height,
        is_cluster: ctx.layout_clusters_by_id.contains_key(id),
        label_width: None,
        label_height: None,
    };
    let start_node = layout_node(&edge.from, &start_bounds);
    let end_node = layout_node(&edge.to, &end_bounds);
    let start_shape = ctx
        .nodes_by_id
        .get(edge.from.as_str())
        .and_then(|node| node.layout_shape.as_deref());
    let end_shape = ctx
        .nodes_by_id
        .get(edge.to.as_str())
        .and_then(|node| node.layout_shape.as_deref());
    let start_intersection = |point: &crate::model::LayoutPoint| {
        intersect_for_layout_shape(ctx, &edge.from, &start_bounds, start_shape, point)
    };
    let end_intersection = |point: &crate::model::LayoutPoint| {
        intersect_for_layout_shape(ctx, &edge.to, &end_bounds, end_shape, point)
    };
    let start = Shape {
        node: &start_node,
        outline: Outline::Rect,
        intersection: Some(&start_intersection),
    };
    let end = Shape {
        node: &end_node,
        outline: Outline::Rect,
        intersection: Some(&end_intersection),
    };
    let mut input = Vec::with_capacity(base_points.len() + 2);
    if has_section && start_shape != Some("rect33") {
        input.push(crate::model::LayoutPoint {
            x: start_bounds.x,
            y: start_bounds.y,
        });
    }
    input.extend_from_slice(base_points);
    if has_section && end_shape != Some("rect33") {
        input.push(crate::model::LayoutPoint {
            x: end_bounds.x,
            y: end_bounds.y,
        });
    }
    *out = geometry::sanitize(&input, start, end);
    let same_point = |a: &crate::model::LayoutPoint, b: &crate::model::LayoutPoint| {
        (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
    };
    let adapters = ElkEndpointAdapterCorners {
        source: has_section
            && !start_node.is_cluster
            && start_shape != Some("rect33")
            && out.len() > 2
            && base_points.first().is_some_and(|p| same_point(&out[1], p)),
        target: has_section
            && !end_node.is_cluster
            && end_shape != Some("rect33")
            && out.len() > 2
            && base_points
                .last()
                .is_some_and(|p| same_point(&out[out.len() - 2], p)),
    };
    if has_section {
        finish_endpoint_adapters(
            out,
            start,
            end,
            adapters,
            edge.edge_type.as_deref(),
            ctx.compact_edge_corners,
        )
    } else {
        adapters
    }
}

fn finish_endpoint_adapters(
    points: &mut Vec<crate::model::LayoutPoint>,
    start: crate::elk_edge_geometry::Shape<'_>,
    end: crate::elk_edge_geometry::Shape<'_>,
    mut adapters: ElkEndpointAdapterCorners,
    edge_type: Option<&str>,
    compact: bool,
) -> ElkEndpointAdapterCorners {
    // Compact corners are a product projection of the sanitized provider route. Align
    // them before marker shortening can discard the port that defines the route axis.
    if compact {
        align_elk_endpoint_adapters_to_route(start, end, &mut adapters, points);
    }
    let source_port = adapters.source.then(|| points[1].clone());
    let target_port = adapters.target.then(|| points[points.len() - 2].clone());
    let (start_marker, end_marker) = super::arrow_types_for_edge(edge_type);
    // Mermaid applies the end adjustment before the start adjustment.
    crate::elk_edge_geometry::marker_segment(points, end, end_marker, false);
    crate::elk_edge_geometry::marker_segment(points, start, start_marker, true);
    // Removing either terminal port can also exhaust the opposite adapter's segment.
    let same_port = |port: Option<&crate::model::LayoutPoint>, index: usize| {
        port.zip(points.get(index))
            .is_some_and(|(a, b)| a.x == b.x && a.y == b.y)
    };
    adapters.source = points.len() > 2 && same_port(source_port.as_ref(), 1);
    adapters.target =
        points.len() > 2 && same_port(target_port.as_ref(), points.len().saturating_sub(2));
    adapters
}

fn align_elk_endpoint_adapters_to_route(
    start: crate::elk_edge_geometry::Shape<'_>,
    end: crate::elk_edge_geometry::Shape<'_>,
    adapters: &mut ElkEndpointAdapterCorners,
    points: &mut Vec<crate::model::LayoutPoint>,
) {
    fn route_intersection(
        shape: crate::elk_edge_geometry::Shape<'_>,
        port: &crate::model::LayoutPoint,
        route_neighbor: &crate::model::LayoutPoint,
    ) -> Option<crate::model::LayoutPoint> {
        let bounds = shape.node;
        let mut dx = route_neighbor.x - port.x;
        let mut dy = route_neighbor.y - port.y;
        let len = dx.hypot(dy);
        if !len.is_finite() || len <= 1e-9 {
            return None;
        }
        dx /= len;
        dy /= len;
        // For either endpoint, the neighboring route point lies away from the node.
        let inward = -1.0;

        let is_inside = |point: &crate::model::LayoutPoint| {
            let boundary = shape.intersect(point);
            let point_distance = (point.x - bounds.x).hypot(point.y - bounds.y);
            let boundary_distance = (boundary.x - bounds.x).hypot(boundary.y - bounds.y);
            if boundary_distance <= 1e-9 {
                return (point.x - bounds.x).abs() <= bounds.width / 2.0
                    && (point.y - bounds.y).abs() <= bounds.height / 2.0;
            }
            point_distance <= boundary_distance + 1e-6
        };

        if is_inside(port) {
            return Some(port.clone());
        }

        let max_distance = (bounds.width + bounds.height).max(1.0) * 2.0;
        let mut outside_distance = 0.0;
        let mut inside_distance = 1.0;
        while inside_distance <= max_distance {
            let candidate = crate::model::LayoutPoint {
                x: port.x + inward * dx * inside_distance,
                y: port.y + inward * dy * inside_distance,
            };
            if is_inside(&candidate) {
                for _ in 0..40 {
                    let mid = (outside_distance + inside_distance) / 2.0;
                    let candidate = crate::model::LayoutPoint {
                        x: port.x + inward * dx * mid,
                        y: port.y + inward * dy * mid,
                    };
                    if is_inside(&candidate) {
                        inside_distance = mid;
                    } else {
                        outside_distance = mid;
                    }
                }
                return Some(crate::model::LayoutPoint {
                    x: port.x + inward * dx * inside_distance,
                    y: port.y + inward * dy * inside_distance,
                });
            }
            outside_distance = inside_distance;
            inside_distance *= 2.0;
        }
        None
    }

    if adapters.source
        && points.len() >= 3
        && let Some(intersection) = route_intersection(start, &points[1], &points[2])
    {
        points[0] = intersection;
        points.remove(1);
        adapters.source = false;
    }

    if adapters.target && points.len() >= 3 {
        let n = points.len();
        if let Some(intersection) = route_intersection(end, &points[n - 2], &points[n - 3]) {
            points[n - 1] = intersection;
            points.remove(n - 2);
            adapters.target = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elk_edge_geometry::{Outline, Shape, sanitize};
    use crate::model::{LayoutNode, LayoutPoint as P};

    fn circle(x: f64) -> LayoutNode {
        LayoutNode {
            id: "circle".into(),
            x,
            y: 0.0,
            width: 20.0,
            height: 20.0,
            is_cluster: false,
            label_width: None,
            label_height: None,
        }
    }

    #[test]
    fn compact_route_alignment_precedes_short_marker_port_removal() {
        let source_node = circle(0.0);
        let target_node = circle(60.0);
        let source = Shape {
            node: &source_node,
            outline: Outline::Ellipse,
            intersection: None,
        };
        let target = Shape {
            node: &target_node,
            outline: Outline::Ellipse,
            intersection: None,
        };
        let provider = vec![
            P { x: 0.0, y: 0.0 },
            P { x: 10.0, y: 3.0 },
            P { x: 20.0, y: 8.0 },
            P { x: 40.0, y: 8.0 },
            P { x: 50.0, y: 3.0 },
            P { x: 60.0, y: 0.0 },
        ];
        for compact in [false, true] {
            let mut points = sanitize(&provider, source, target);
            // Mermaid cutter2 pops the final intersection when its distance to the
            // target port is below 2. The surviving port is an endpoint, not an adapter.
            assert_eq!(points.len(), 5);
            let same_point = |a: &P, b: &P| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6;
            let adapters = ElkEndpointAdapterCorners {
                source: same_point(&points[1], &provider[1]),
                target: same_point(&points[points.len() - 2], &provider[4]),
            };
            assert!(adapters.source);
            assert!(!adapters.target);
            let adapters = finish_endpoint_adapters(
                &mut points,
                source,
                target,
                adapters,
                Some("double_arrow_point"),
                compact,
            );
            let (x, y) = if compact {
                (9.6, 2.8)
            } else {
                let radius_scale = 10.0 / 109.0_f64.sqrt();
                (10.0 * radius_scale, 3.0 * radius_scale)
            };
            assert_eq!(points.len(), 4);
            assert!((points[0].x - x).abs() < 2e-6, "{points:?}");
            assert!((points[0].y - y).abs() < 2e-6, "{points:?}");
            assert_eq!((points[3].x, points[3].y), (50.0, 3.0));
            assert_eq!((points[1].x, points[1].y), (20.0, 8.0));
            assert_eq!((points[2].x, points[2].y), (40.0, 8.0));
            assert!(!adapters.source && !adapters.target);
        }
        assert_eq!(provider.len(), 6);
        assert_eq!((provider[1].x, provider[1].y), (10.0, 3.0));
    }

    #[test]
    fn marker_removal_rechecks_adapter_indices_after_each_end_changes() {
        let source_node = circle(0.0);
        let target_node = circle(40.0);
        let source = Shape {
            node: &source_node,
            outline: Outline::Ellipse,
            intersection: None,
        };
        let target = Shape {
            node: &target_node,
            outline: Outline::Ellipse,
            intersection: None,
        };
        let mut points = vec![
            P { x: 9.6, y: 2.8 },
            P { x: 10.0, y: 3.0 },
            P { x: 30.0, y: 3.0 },
            P { x: 30.4, y: 2.8 },
        ];
        let adapters = finish_endpoint_adapters(
            &mut points,
            source,
            target,
            ElkEndpointAdapterCorners {
                source: true,
                target: true,
            },
            Some("double_arrow_point"),
            false,
        );
        assert_eq!(points.len(), 2);
        assert_eq!((points[0].x, points[0].y), (9.6, 2.8));
        assert_eq!((points[1].x, points[1].y), (30.4, 2.8));
        assert!(!adapters.source && !adapters.target);
    }
}
