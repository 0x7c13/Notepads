//! Mermaid `lineWithOffset` port.
//!
//! Mermaid shortens edge paths so markers don't render on top of the line (see
//! `packages/mermaid/src/utils/lineWithOffset.ts`).

use super::BoundaryNode;
use crate::svg::parity::edge_path::{
    line_with_offset_points, marker_offset_for, rounded_line_with_marker_offsets_points,
};

pub(in crate::svg::parity::flowchart) fn collapse_short_terminal_marker_stub(
    points: &mut Vec<crate::model::LayoutPoint>,
    edge_type: Option<&str>,
    target_boundary: Option<BoundaryNode>,
) -> bool {
    let (_, arrow_type_end) = arrow_types_for_edge(edge_type);
    let Some(offset) = marker_offset_for(arrow_type_end) else {
        return false;
    };
    if points.len() < 3 {
        return false;
    }

    let n = points.len();
    let incoming_x = points[n - 2].x - points[n - 3].x;
    let incoming_y = points[n - 2].y - points[n - 3].y;
    let terminal_x = points[n - 1].x - points[n - 2].x;
    let terminal_y = points[n - 1].y - points[n - 2].y;
    let terminal_len = terminal_x.hypot(terminal_y);

    if let Some(boundary) = target_boundary {
        let min_segment_length = 8.0_f64.max(offset * 2.0);
        if terminal_len >= min_segment_length
            || !point_is_on_rect_border(&boundary, &points[n - 2], 1.0)
        {
            return false;
        }

        points.remove(n - 2);
        return true;
    }

    let continues_forward = incoming_x * terminal_x + incoming_y * terminal_y >= 0.0;

    if terminal_len > 0.0 && terminal_len <= offset && continues_forward {
        points.remove(n - 2);
        true
    } else {
        false
    }
}

fn point_is_on_rect_border(
    boundary: &BoundaryNode,
    point: &crate::model::LayoutPoint,
    tolerance: f64,
) -> bool {
    let half_width = boundary.width / 2.0;
    let half_height = boundary.height / 2.0;
    let left = boundary.x - half_width;
    let right = boundary.x + half_width;
    let top = boundary.y - half_height;
    let bottom = boundary.y + half_height;

    let on_left = (point.x - left).abs() <= tolerance
        && point.y >= top - tolerance
        && point.y <= bottom + tolerance;
    let on_right = (point.x - right).abs() <= tolerance
        && point.y >= top - tolerance
        && point.y <= bottom + tolerance;
    let on_top = (point.y - top).abs() <= tolerance
        && point.x >= left - tolerance
        && point.x <= right + tolerance;
    let on_bottom = (point.y - bottom).abs() <= tolerance
        && point.x >= left - tolerance
        && point.x <= right + tolerance;

    on_left || on_right || on_top || on_bottom
}

pub(in crate::svg::parity::flowchart) fn arrow_types_for_edge(
    edge_type: Option<&str>,
) -> (Option<&'static str>, Option<&'static str>) {
    let arrow_type_start = match edge_type {
        Some("double_arrow_point") => Some("arrow_point"),
        Some("double_arrow_circle") => Some("arrow_circle"),
        Some("double_arrow_cross") => Some("arrow_cross"),
        _ => None,
    };
    let arrow_type_end = match edge_type {
        Some("arrow_open") => None,
        Some("arrow_cross") => Some("arrow_cross"),
        Some("arrow_circle") => Some("arrow_circle"),
        Some("double_arrow_point" | "arrow_point") => Some("arrow_point"),
        Some("double_arrow_circle") => Some("arrow_circle"),
        Some("double_arrow_cross") => Some("arrow_cross"),
        _ => Some("arrow_point"),
    };

    (arrow_type_start, arrow_type_end)
}

pub(in crate::svg::parity::flowchart) fn line_with_offset_for_edge_type(
    input: &[crate::model::LayoutPoint],
    edge_type: Option<&str>,
) -> Vec<crate::model::LayoutPoint> {
    let (arrow_type_start, arrow_type_end) = arrow_types_for_edge(edge_type);
    line_with_offset_points(input, arrow_type_start, arrow_type_end)
}

pub(in crate::svg::parity::flowchart) fn rounded_line_with_marker_offsets_for_edge_type(
    input: &[crate::model::LayoutPoint],
    edge_type: Option<&str>,
) -> Vec<crate::model::LayoutPoint> {
    let (arrow_type_start, arrow_type_end) = arrow_types_for_edge(edge_type);
    rounded_line_with_marker_offsets_points(input, arrow_type_start, arrow_type_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_marker_geometry_collapses_only_short_forward_terminal_stubs() {
        let mut short = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 20.0, y: 0.0 },
            crate::model::LayoutPoint { x: 22.0, y: 1.0 },
        ];
        assert!(collapse_short_terminal_marker_stub(
            &mut short,
            Some("arrow_point"),
            None,
        ));
        assert_eq!(short.len(), 2);

        let rendered = rounded_line_with_marker_offsets_points(&short, None, Some("arrow_point"));
        let final_dx = rendered[1].x - rendered[0].x;
        let final_dy = rendered[1].y - rendered[0].y;
        assert!(final_dx > 0.0);
        assert!(final_dx * 22.0 + final_dy > 0.0);

        let mut long = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 20.0, y: 0.0 },
            crate::model::LayoutPoint { x: 30.0, y: 1.0 },
        ];
        assert!(!collapse_short_terminal_marker_stub(
            &mut long,
            Some("arrow_point"),
            None,
        ));
        assert_eq!(long.len(), 3);
    }

    #[test]
    fn elk_marker_clearance_only_removes_short_target_border_entries() {
        let target = BoundaryNode {
            x: 25.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        };
        let mut short_border_entry = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 20.0, y: 0.0 },
            crate::model::LayoutPoint { x: 25.0, y: 0.0 },
        ];
        assert!(collapse_short_terminal_marker_stub(
            &mut short_border_entry,
            Some("arrow_point"),
            Some(target),
        ));
        assert_eq!(short_border_entry.len(), 2);

        let mut short_real_bend = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 18.0, y: 0.0 },
            crate::model::LayoutPoint { x: 22.0, y: 0.0 },
        ];
        assert!(!collapse_short_terminal_marker_stub(
            &mut short_real_bend,
            Some("arrow_point"),
            Some(target),
        ));
        assert_eq!(short_real_bend.len(), 3);

        let mut sufficient_runway = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 20.0, y: 0.0 },
            crate::model::LayoutPoint { x: 28.0, y: 0.0 },
        ];
        assert!(!collapse_short_terminal_marker_stub(
            &mut sufficient_runway,
            Some("arrow_point"),
            Some(target),
        ));
        assert_eq!(sufficient_runway.len(), 3);
    }
}
