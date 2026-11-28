use crate::model::LayoutPoint;

fn js_round(value: f64, precision: i32) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    let factor = 10_f64.powi(precision);
    let scaled = value * factor;
    if !factor.is_finite() || !scaled.is_finite() {
        return None;
    }
    let lower = scaled.floor();
    let rounded_integer = if scaled - lower < 0.5 {
        lower
    } else {
        lower + 1.0
    };
    let rounded_integer = if rounded_integer == 0.0 && scaled.is_sign_negative() {
        -0.0
    } else {
        rounded_integer
    };
    let rounded = rounded_integer / factor;
    rounded.is_finite().then_some(rounded)
}

/// Mirrors Mermaid 11.16 `utils.calcLabelPosition` for finite SVG geometry.
pub(super) fn calc_label_position(label_path_points: &[LayoutPoint]) -> Option<LayoutPoint> {
    if label_path_points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return None;
    }

    match label_path_points {
        [] => return None,
        [point] => return Some(point.clone()),
        _ => {}
    }

    let total_distance = label_path_points
        .windows(2)
        .map(|segment| {
            let dx = segment[1].x - segment[0].x;
            let dy = segment[1].y - segment[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .sum::<f64>();
    if !total_distance.is_finite() {
        return None;
    }

    let mut remaining_distance = total_distance / 2.0;
    for segment in label_path_points.windows(2) {
        let previous = &segment[0];
        let point = &segment[1];
        let dx = point.x - previous.x;
        let dy = point.y - previous.y;
        let vector_distance = (dx * dx + dy * dy).sqrt();
        if vector_distance == 0.0 {
            return Some(previous.clone());
        }
        if vector_distance < remaining_distance {
            remaining_distance -= vector_distance;
            continue;
        }

        let ratio = remaining_distance / vector_distance;
        if ratio <= 0.0 {
            return Some(previous.clone());
        }
        if ratio >= 1.0 {
            return Some(point.clone());
        }
        return Some(LayoutPoint {
            x: js_round((1.0 - ratio) * previous.x + ratio * point.x, 5)?,
            y: js_round((1.0 - ratio) * previous.y + ratio * point.y, 5)?,
        });
    }

    None
}

pub(super) fn is_label_coordinate_in_path(point: &LayoutPoint, d_attr: &str) -> bool {
    let Some(rounded_x) = js_rounded_number_string(point.x) else {
        return false;
    };
    let Some(rounded_y) = js_rounded_number_string(point.y) else {
        return false;
    };
    let Some(sanitized) = round_decimal_numbers_in_path(d_attr) else {
        return false;
    };
    sanitized.contains(&rounded_x) || sanitized.contains(&rounded_y)
}

/// Mermaid 12.1 keeps layout label offsets when paint-time clipping moves the route.
pub(super) fn position_edge_label(
    dagre_anchor: LayoutPoint,
    original_path_points: Option<&[LayoutPoint]>,
    label_path_points: &[LayoutPoint],
    rendered_d: &str,
    points_were_explicitly_updated: bool,
) -> LayoutPoint {
    let path_was_updated = points_were_explicitly_updated
        || label_path_points
            .get(label_path_points.len() / 2)
            .is_some_and(|midpoint| !is_label_coordinate_in_path(midpoint, rendered_d));
    if !path_was_updated {
        return dagre_anchor;
    }

    let Some(updated_midpoint) = calc_label_position(label_path_points) else {
        return dagre_anchor;
    };
    let Some(original_path_points) = original_path_points else {
        return updated_midpoint;
    };
    let Some(original_midpoint) = calc_label_position(original_path_points) else {
        return dagre_anchor;
    };
    LayoutPoint {
        x: dagre_anchor.x + (updated_midpoint.x - original_midpoint.x),
        y: dagre_anchor.y + (updated_midpoint.y - original_midpoint.y),
    }
}

fn js_rounded_number_string(value: f64) -> Option<String> {
    let mut rounded = js_round(value, 0)?;
    if rounded == -0.0 {
        rounded = 0.0;
    }
    Some(ryu_js::Buffer::new().format_finite(rounded).to_string())
}

fn round_decimal_numbers_in_path(d_attr: &str) -> Option<String> {
    let mut out = String::new();
    let mut copied_until = 0usize;
    let mut cursor = 0usize;
    let mut changed = false;

    while cursor < d_attr.len() {
        if let Some(end) = decimal_number_match_end_at(d_attr, cursor) {
            if !changed {
                out = String::with_capacity(d_attr.len());
                changed = true;
            }
            out.push_str(&d_attr[copied_until..cursor]);
            let value = d_attr[cursor..end].parse::<f64>().ok()?;
            out.push_str(&js_rounded_number_string(value)?);
            copied_until = end;
            cursor = end;
            continue;
        }

        let Some(character) = d_attr[cursor..].chars().next() else {
            break;
        };
        cursor += character.len_utf8();
    }

    if changed {
        out.push_str(&d_attr[copied_until..]);
        Some(out)
    } else {
        Some(d_attr.to_string())
    }
}

fn decimal_number_match_end_at(value: &str, start: usize) -> Option<usize> {
    let digit_start = start;
    let mut cursor = consume_ascii_digits(value, start);
    if cursor == digit_start || !value.get(cursor..)?.starts_with('.') {
        return None;
    }

    let fraction_start = cursor + 1;
    cursor = consume_ascii_digits(value, fraction_start);
    (cursor != fraction_start).then_some(cursor)
}

fn consume_ascii_digits(value: &str, mut cursor: usize) -> usize {
    while value.as_bytes().get(cursor).is_some_and(u8::is_ascii_digit) {
        cursor += 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_point(actual: Option<LayoutPoint>, expected_x: f64, expected_y: f64) {
        let actual = actual.expect("expected a label point");
        assert_eq!(actual.x, expected_x);
        assert_eq!(actual.y, expected_y);
    }

    #[test]
    fn requirement_curve_midpoint_matches_mermaid_11_16() {
        let points = [
            LayoutPoint {
                x: 290.96875,
                y: 381.8802782,
            },
            LayoutPoint {
                x: 228.296875,
                y: 439.0,
            },
            LayoutPoint {
                x: 199.9138505,
                y: 476.0,
            },
        ];

        assert_point(calc_label_position(&points), 242.40006, 426.14623);
    }

    #[test]
    fn updated_path_keeps_layout_anchor_when_the_route_midpoint_is_unchanged() {
        let points = [
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint { x: 10.0, y: 0.0 },
            LayoutPoint { x: 20.0, y: 0.0 },
        ];
        let anchor = LayoutPoint { x: 4.0, y: 5.0 };

        let unchanged = position_edge_label(
            anchor.clone(),
            Some(&points),
            &points,
            "M0,0 L10,0 L20,0",
            false,
        );
        assert_eq!((unchanged.x, unchanged.y), (anchor.x, anchor.y));
        let updated = position_edge_label(anchor, Some(&points), &points, "M0,0 L10,0 L20,0", true);
        assert_eq!((updated.x, updated.y), (4.0, 5.0));
    }

    #[test]
    fn updated_path_translates_the_anchor_by_the_actual_midpoint_delta() {
        let original = [
            LayoutPoint { x: -20.0, y: 0.0 },
            LayoutPoint { x: 100.0, y: 0.0 },
        ];
        let updated = [
            LayoutPoint { x: 0.0, y: 10.0 },
            LayoutPoint { x: 100.0, y: 10.0 },
        ];
        let position = position_edge_label(
            LayoutPoint { x: 12.0, y: 8.0 },
            Some(&original),
            &updated,
            "M0,10L100,10",
            true,
        );
        assert_eq!((position.x, position.y), (22.0, 18.0));
        let without_original = position_edge_label(
            LayoutPoint { x: 12.0, y: 8.0 },
            None,
            &updated,
            "M0,10L100,10",
            true,
        );
        assert_eq!((without_original.x, without_original.y), (50.0, 10.0));
    }

    #[test]
    fn diagonal_marker_shortening_preserves_the_separated_label_anchor() {
        let points = [
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint { x: 100.0, y: 100.0 },
        ];
        let rendered_d = "M0,0L96.111,96.111";
        assert!(!is_label_coordinate_in_path(&points[1], rendered_d));
        let position = position_edge_label(
            LayoutPoint { x: 60.0, y: 50.0 },
            Some(&points),
            &points,
            rendered_d,
            false,
        );
        assert_eq!((position.x, position.y), (60.0, 50.0));
    }

    #[test]
    fn calculation_handles_empty_single_and_zero_length_paths() {
        assert!(calc_label_position(&[]).is_none());
        assert_point(
            calc_label_position(&[LayoutPoint { x: 2.0, y: 3.0 }]),
            2.0,
            3.0,
        );
        assert_point(
            calc_label_position(&[
                LayoutPoint { x: 2.0, y: 3.0 },
                LayoutPoint { x: 2.0, y: 3.0 },
            ]),
            2.0,
            3.0,
        );
    }

    #[test]
    fn interpolation_uses_js_round_but_exact_endpoints_are_not_rounded() {
        assert_point(
            calc_label_position(&[
                LayoutPoint {
                    x: -0.00001,
                    y: 0.0,
                },
                LayoutPoint { x: 0.0, y: 0.0 },
            ]),
            0.0,
            0.0,
        );
        assert_point(
            calc_label_position(&[
                LayoutPoint { x: 0.0, y: 0.0 },
                LayoutPoint {
                    x: 1.234567,
                    y: 0.0,
                },
                LayoutPoint {
                    x: 2.469134,
                    y: 0.0,
                },
            ]),
            1.234567,
            0.0,
        );
    }

    #[test]
    fn path_coordinate_detection_preserves_mermaids_string_heuristic() {
        let point = LayoutPoint { x: 22.0, y: 99.0 };
        assert!(is_label_coordinate_in_path(&point, "M122 0"));
        assert!(is_label_coordinate_in_path(
            &LayoutPoint {
                x: 9_007_199_254_740_994.0,
                y: 22.0,
            },
            "M122 0"
        ));
        assert!(is_label_coordinate_in_path(
            &LayoutPoint { x: 12.0, y: 99.0 },
            "M-12.4 0"
        ));
        assert_eq!(
            round_decimal_numbers_in_path("M1 .5 10."),
            Some("M1 .5 10.".to_string())
        );
        assert!(!is_label_coordinate_in_path(
            &LayoutPoint { x: 7.0, y: 8.0 },
            "M1 .5 10."
        ));
    }

    #[test]
    fn negative_half_rounding_preserves_the_unsigned_decimal_regex_quirk() {
        assert_eq!(js_round(0.49999999999999994, 0), Some(0.0));
        assert_eq!(js_round(0.5, 0), Some(1.0));
        assert_eq!(js_round(-10.5, 0), Some(-10.0));
        assert!(js_round(-0.1, 0).is_some_and(f64::is_sign_negative));
        assert_eq!(
            round_decimal_numbers_in_path("M-10.5 20.6"),
            Some("M-11 21".to_string())
        );
        assert!(!is_label_coordinate_in_path(
            &LayoutPoint { x: -10.5, y: 99.0 },
            "M-10.5 20.6"
        ));
    }

    #[test]
    fn non_finite_geometry_fails_closed() {
        let anchor = LayoutPoint { x: 4.0, y: 5.0 };
        let points = [
            LayoutPoint { x: 0.0, y: 0.0 },
            LayoutPoint {
                x: f64::NAN,
                y: 1.0,
            },
        ];

        assert!(calc_label_position(&points).is_none());
        assert!(!is_label_coordinate_in_path(
            &LayoutPoint {
                x: f64::INFINITY,
                y: 0.0,
            },
            "M0 0L1 1"
        ));
        let overflowing_decimal = format!("M{}.0 0", "9".repeat(400));
        assert!(!is_label_coordinate_in_path(
            &LayoutPoint { x: 9.0, y: 8.0 },
            &overflowing_decimal
        ));
        let position =
            position_edge_label(anchor.clone(), Some(&points), &points, "M0 0L1 1", true);
        assert_eq!((position.x, position.y), (anchor.x, anchor.y));
    }
}
