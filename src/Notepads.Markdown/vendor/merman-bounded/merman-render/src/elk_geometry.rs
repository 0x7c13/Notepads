//! Mermaid 12 missing-section geometry shared by Class/ER label placement and SVG paint.

use crate::model::{LayoutNode, LayoutPoint};

/// Mirrors the two-center path through `sanitizeElkEdgePoints` in the pinned ELK renderer.
/// This derived geometry must not replace the raw provider route: empty sections differ
/// from two real points and select different curve and label behavior in the renderer.
pub(crate) fn missing_rect_section_points(
    start: &LayoutNode,
    end: &LayoutNode,
) -> Vec<LayoutPoint> {
    fn intersect(node: &LayoutNode, point: &LayoutPoint) -> LayoutPoint {
        let dx = point.x - node.x;
        let dy = point.y - node.y;
        let w = node.width / 2.0;
        let h = node.height / 2.0;
        let (sx, sy) = if dy.abs() * w > dx.abs() * h {
            let h = if dy < 0.0 { -h } else { h };
            (if dy == 0.0 { 0.0 } else { h * dx / dy }, h)
        } else {
            let w = if dx < 0.0 { -w } else { w };
            (w, if dx == 0.0 { 0.0 } else { w * dy / dx })
        };
        LayoutPoint {
            x: node.x + sx,
            y: node.y + sy,
        }
    }

    let centers = [
        LayoutPoint {
            x: start.x,
            y: start.y,
        },
        LayoutPoint { x: end.x, y: end.y },
    ];
    let mut points = centers.clone();
    if (end.x - start.x).abs() >= start.width / 2.0 || (end.y - start.y).abs() >= start.height / 2.0
    {
        points[0] = intersect(start, &centers[1]);
    }
    points[1] = intersect(end, &points[0]);
    // The source removes a tail closer than 2px, then restores the original centers if
    // fewer than two valid points survive. Consecutive deduplication runs afterwards.
    if (points[1].x - points[0].x).hypot(points[1].y - points[0].y) < 2.0
        || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        points = centers;
    }
    if (points[1].x - points[0].x).abs() <= 1e-6 && (points[1].y - points[0].y).abs() <= 1e-6 {
        vec![points[0].clone()]
    } else {
        points.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::missing_rect_section_points;
    use crate::model::LayoutNode;

    #[test]
    fn overlapping_nodes_keep_a_drawable_end_intersection() {
        let start = LayoutNode {
            id: "A".into(),
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
            is_cluster: false,
            label_width: None,
            label_height: None,
        };
        let end = LayoutNode {
            id: "B".into(),
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 20.0,
            is_cluster: false,
            label_width: None,
            label_height: None,
        };

        let points = missing_rect_section_points(&start, &end);
        assert_eq!(points.len(), 2);
        assert!((points[0].x - 10.0).abs() < f64::EPSILON);
        assert!((points[0].y - 10.0).abs() < f64::EPSILON);
        assert!((points[1].x - 30.0).abs() < f64::EPSILON);
        assert!((points[1].y - 10.0).abs() < f64::EPSILON);

        // Touching rectangles intersect at the same border point. The source
        // restores centers for that short route instead of choosing a new border.
        let touching = LayoutNode { x: 50.0, ..end };
        let points = missing_rect_section_points(&start, &touching);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].x, 10.0);
        assert_eq!(points[1].x, 50.0);

        // A tiny coincident node reaches the center fallback and final dedup.
        let tiny = LayoutNode {
            width: 1.0,
            height: 1.0,
            ..start
        };
        let points = missing_rect_section_points(&tiny, &tiny);
        assert_eq!(points.len(), 1);
        assert_eq!((points[0].x, points[0].y), (10.0, 10.0));
    }
}
