//! Shared Mermaid edge marker offsets and corner preparation.

mod fix_corners;
pub(super) use fix_corners::maybe_fix_corners;

/// Mirrors Mermaid `edges.js`'s static Neo marker mask, independently of path clipping.
/// Class, State, ER, and Requirement markers have no entry in `markerOffsets2`.
pub(super) fn write_neo_edge_mask(
    out: &mut String,
    path_length: f64,
    arrow_type_start: Option<&str>,
    arrow_type_end: Option<&str>,
    dashed: bool,
    line_hop_applied: bool,
) {
    use std::fmt::Write as _;

    fn mask_offset(arrow_type: Option<&str>) -> f64 {
        match arrow_type {
            Some("arrow_point") => 4.0,
            Some("arrow_cross" | "arrow_circle") => 12.5,
            _ => 0.0,
        }
    }

    let start_offset = mask_offset(arrow_type_start);
    let end_offset = mask_offset(arrow_type_end);
    let middle_length = if line_hop_applied {
        (path_length - start_offset - end_offset).max(0.0)
    } else {
        path_length - start_offset - end_offset
    };
    out.push_str("stroke-dasharray: 0 ");
    let _ = write!(out, "{} ", super::fmt(start_offset));
    if dashed {
        let pairs = (middle_length / 4.0).floor();
        let pairs = if pairs.is_finite() {
            pairs.max(0.0) as usize
        } else {
            0
        };
        for _ in 0..pairs {
            out.push_str("2 2 ");
        }
    } else {
        let _ = write!(out, "{} ", super::fmt(middle_length));
    }
    let _ = write!(out, "{}; stroke-dashoffset: 0;", super::fmt(end_offset));
}

pub(super) fn marker_offset_for(arrow_type: Option<&str>) -> Option<f64> {
    match arrow_type {
        Some("arrow_point") => Some(4.0),
        Some("dependency") => Some(6.0),
        Some("lollipop") => Some(13.5),
        Some("aggregation" | "extension" | "composition") => Some(17.25),
        _ => None,
    }
}

pub(super) fn line_with_offset_points(
    input: &[crate::model::LayoutPoint],
    arrow_type_start: Option<&str>,
    arrow_type_end: Option<&str>,
) -> Vec<crate::model::LayoutPoint> {
    fn calculate_delta_and_angle(
        a: &crate::model::LayoutPoint,
        b: &crate::model::LayoutPoint,
    ) -> (f64, f64, f64) {
        let delta_x = b.x - a.x;
        let delta_y = b.y - a.y;
        let angle = (delta_y / delta_x).atan();
        (angle, delta_x, delta_y)
    }

    if input.len() < 2 {
        return input.to_vec();
    }

    let start = &input[0];
    let end = &input[input.len() - 1];

    let x_direction_is_left = start.x < end.x;
    let y_direction_is_down = start.y < end.y;
    let extra_room = 1.0;

    let start_marker_height = marker_offset_for(arrow_type_start);
    let end_marker_height = marker_offset_for(arrow_type_end);

    let mut out = Vec::with_capacity(input.len());
    for (i, p) in input.iter().enumerate() {
        let mut ox = 0.0;
        let mut oy = 0.0;

        if i == 0 {
            if let Some(h) = start_marker_height {
                let (angle, delta_x, delta_y) = calculate_delta_and_angle(&input[0], &input[1]);
                ox = h * angle.cos() * if delta_x >= 0.0 { 1.0 } else { -1.0 };
                oy = h * angle.sin().abs() * if delta_y >= 0.0 { 1.0 } else { -1.0 };
            }
        } else if i == input.len() - 1
            && let Some(h) = end_marker_height
        {
            let (angle, delta_x, delta_y) =
                calculate_delta_and_angle(&input[input.len() - 1], &input[input.len() - 2]);
            ox = h * angle.cos() * if delta_x >= 0.0 { 1.0 } else { -1.0 };
            oy = h * angle.sin().abs() * if delta_y >= 0.0 { 1.0 } else { -1.0 };
        }

        if let Some(h) = end_marker_height {
            let diff_x = (p.x - end.x).abs();
            let diff_y = (p.y - end.y).abs();
            if diff_x < h && diff_x > 0.0 && diff_y < h {
                let mut adjustment = h + extra_room - diff_x;
                adjustment *= if !x_direction_is_left { -1.0 } else { 1.0 };
                ox -= adjustment;
            }
        }
        if let Some(h) = start_marker_height {
            let diff_x = (p.x - start.x).abs();
            let diff_y = (p.y - start.y).abs();
            if diff_x < h && diff_x > 0.0 && diff_y < h {
                let mut adjustment = h + extra_room - diff_x;
                adjustment *= if !x_direction_is_left { -1.0 } else { 1.0 };
                ox += adjustment;
            }
        }

        if let Some(h) = end_marker_height {
            let diff_y = (p.y - end.y).abs();
            let diff_x = (p.x - end.x).abs();
            if diff_y < h && diff_y > 0.0 && diff_x < h {
                let mut adjustment = h + extra_room - diff_y;
                adjustment *= if !y_direction_is_down { -1.0 } else { 1.0 };
                oy -= adjustment;
            }
        }
        if let Some(h) = start_marker_height {
            let diff_y = (p.y - start.y).abs();
            let diff_x = (p.x - start.x).abs();
            if diff_y < h && diff_y > 0.0 && diff_x < h {
                let mut adjustment = h + extra_room - diff_y;
                adjustment *= if !y_direction_is_down { -1.0 } else { 1.0 };
                oy += adjustment;
            }
        }

        out.push(crate::model::LayoutPoint {
            x: p.x + ox,
            y: p.y + oy,
        });
    }

    out
}

pub(super) fn rounded_line_with_marker_offsets_points(
    input: &[crate::model::LayoutPoint],
    arrow_type_start: Option<&str>,
    arrow_type_end: Option<&str>,
) -> Vec<crate::model::LayoutPoint> {
    let mut out = input.to_vec();
    if input.len() < 2 {
        return out;
    }

    if let Some(offset) = marker_offset_for(arrow_type_start) {
        let p1 = &input[0];
        let p2 = &input[1];
        let angle = (p2.y - p1.y).atan2(p2.x - p1.x);
        out[0].x = p1.x + offset * angle.cos();
        out[0].y = p1.y + offset * angle.sin();
    }

    let n = input.len();
    if let Some(offset) = marker_offset_for(arrow_type_end) {
        let p1 = &input[n - 1];
        let p2 = &input[n - 2];
        let angle = (p1.y - p2.y).atan2(p1.x - p2.x);
        out[n - 1].x = p1.x - offset * angle.cos();
        out[n - 1].y = p1.y - offset * angle.sin();
    }

    out
}

/// The shared renderer resolves a Usecase Dagre edge through flowchart.curve; ELK supplies
/// rounded for routed sections and linear when a provider returns no sections.
pub(super) fn render_path(
    points: &mut Vec<crate::model::LayoutPoint>,
    kind: &str,
    start_marker: Option<&str>,
    end_marker: Option<&str>,
) -> String {
    use super::curve;
    if kind == "rounded" {
        let points = rounded_line_with_marker_offsets_points(points, start_marker, end_marker);
        return curve::curve_rounded_path_d_and_bounds(&points, 5.0, false, None).0;
    }
    maybe_fix_corners(points);
    let points = line_with_offset_points(points, start_marker, end_marker);
    match kind {
        "linear" => curve::curve_linear_path_d(&points),
        "natural" => curve::curve_natural_path_d_and_bounds(&points).0,
        "bumpX" => curve::curve_bump_x_path_d_and_bounds(&points).0,
        "bumpY" => curve::curve_bump_y_path_d_and_bounds(&points).0,
        "catmullRom" => curve::curve_catmull_rom_path_d_and_bounds(&points).0,
        "step" => curve::curve_step_path_d_and_bounds(&points).0,
        "stepAfter" => curve::curve_step_after_path_d_and_bounds(&points).0,
        "stepBefore" => curve::curve_step_before_path_d_and_bounds(&points).0,
        "cardinal" => curve::curve_cardinal_path_d_and_bounds(&points, 0.0).0,
        "monotoneX" => curve::curve_monotone_path_d_and_bounds(&points, false).0,
        "monotoneY" => curve::curve_monotone_path_d_and_bounds(&points, true).0,
        _ => curve::curve_basis_path_d(&points),
    }
}
