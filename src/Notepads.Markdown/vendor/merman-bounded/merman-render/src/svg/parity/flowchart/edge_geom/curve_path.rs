//! Flowchart edge curve selection and bounds.
//!
//! This logic is shared between flowchart SVG emission and viewBox computation, and mirrors the
//! behavior of upstream Mermaid when selecting the D3 curve implementation and when deciding
//! whether curve bounds can be skipped for `basis`.

use super::*;

pub(in crate::svg::parity::flowchart) fn curve_path_d_and_bounds(
    line_data: &[crate::model::LayoutPoint],
    interpolate: &str,
    rounded_radius: f64,
    compact_edge_corners: bool,
    rounded_corner_mask: Option<&[bool]>,
) -> (String, Option<path_bounds::SvgPathBounds>, bool) {
    let curve_is_basis = !matches!(
        interpolate,
        "linear"
            | "natural"
            | "bumpX"
            | "bumpY"
            | "catmullRom"
            | "step"
            | "stepAfter"
            | "stepBefore"
            | "cardinal"
            | "monotoneX"
            | "monotoneY"
            | "rounded"
    );

    if curve_is_basis {
        let (d, raw_pb) = crate::svg::parity::curve::curve_basis_path_d_and_bounds(line_data);
        let d = maybe_close_single_point_path(d, line_data);
        let pb = svg_path_bounds_from_d(&d).or(raw_pb);
        (d, pb, false)
    } else {
        let (d, pb) = match interpolate {
            "linear" => crate::svg::parity::curve::curve_linear_path_d_and_bounds(line_data),
            "natural" => crate::svg::parity::curve::curve_natural_path_d_and_bounds(line_data),
            "bumpX" => crate::svg::parity::curve::curve_bump_x_path_d_and_bounds(line_data),
            "bumpY" => crate::svg::parity::curve::curve_bump_y_path_d_and_bounds(line_data),
            "catmullRom" => {
                crate::svg::parity::curve::curve_catmull_rom_path_d_and_bounds(line_data)
            }
            "step" => crate::svg::parity::curve::curve_step_path_d_and_bounds(line_data),
            "stepAfter" => crate::svg::parity::curve::curve_step_after_path_d_and_bounds(line_data),
            "stepBefore" => {
                crate::svg::parity::curve::curve_step_before_path_d_and_bounds(line_data)
            }
            "cardinal" => {
                crate::svg::parity::curve::curve_cardinal_path_d_and_bounds(line_data, 0.0)
            }
            "monotoneX" => {
                crate::svg::parity::curve::curve_monotone_path_d_and_bounds(line_data, false)
            }
            "monotoneY" => {
                crate::svg::parity::curve::curve_monotone_path_d_and_bounds(line_data, true)
            }
            "rounded" => crate::svg::parity::curve::curve_rounded_path_d_and_bounds(
                line_data,
                rounded_radius,
                compact_edge_corners,
                rounded_corner_mask,
            ),
            // Unknown curve names fall back to Mermaid's historical `basis` behavior.
            _ => crate::svg::parity::curve::curve_basis_path_d_and_bounds(line_data),
        };

        let d = maybe_close_single_point_path(d, line_data);
        let pb = svg_path_bounds_from_d(&d).or(pb);
        (d, pb, false)
    }
}

fn maybe_close_single_point_path(d: String, line_data: &[crate::model::LayoutPoint]) -> String {
    if line_data.len() == 1 && !d.ends_with('Z') {
        let mut d = d;
        d.push('Z');
        d
    } else {
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_two_points_stays_linear_like_d3() {
        let points = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 10.0, y: 4.0 },
        ];
        let (path, _, _) = curve_path_d_and_bounds(&points, "basis", 5.0, false, None);
        assert_eq!(path, "M0,0L10,4");
    }

    #[test]
    fn bump_axes_match_d3_reference_paths() {
        let points = vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: 20.0, y: 10.0 },
            crate::model::LayoutPoint { x: 40.0, y: 0.0 },
        ];
        // d3-shape from the Mermaid 12 reference graph, line().curve(curveBumpX/Y).
        for (curve, expected) in [
            ("bumpX", "M0,0C10,0,10,10,20,10C30,10,30,0,40,0"),
            ("bumpY", "M0,0C0,5,20,5,20,10C20,5,40,5,40,0"),
        ] {
            let (path, bounds, _) = curve_path_d_and_bounds(&points, curve, 5.0, false, None);
            assert_eq!(path, expected);
            assert!(bounds.is_some());
        }
    }

    #[test]
    fn maybe_close_single_point_path_appends_z_once() {
        let line_data = vec![crate::model::LayoutPoint { x: 1.0, y: 2.0 }];

        assert_eq!(
            maybe_close_single_point_path("M1,2".to_string(), &line_data),
            "M1,2Z"
        );
        assert_eq!(
            maybe_close_single_point_path("M1,2Z".to_string(), &line_data),
            "M1,2Z"
        );
    }

    #[test]
    fn maybe_close_single_point_path_preserves_multi_point_paths() {
        let line_data = vec![
            crate::model::LayoutPoint { x: 1.0, y: 2.0 },
            crate::model::LayoutPoint { x: 3.0, y: 4.0 },
        ];

        assert_eq!(
            maybe_close_single_point_path("M1,2L3,4".to_string(), &line_data),
            "M1,2L3,4"
        );
    }

    #[test]
    fn compact_rounded_corners_do_not_overrun_shallow_elk_endpoint_turns() {
        let line_data = vec![
            crate::model::LayoutPoint {
                x: 130.484,
                y: 214.303,
            },
            crate::model::LayoutPoint {
                x: 135.784,
                y: 230.203,
            },
            crate::model::LayoutPoint {
                x: 135.784,
                y: 250.203,
            },
            crate::model::LayoutPoint {
                x: 260.023,
                y: 250.203,
            },
        ];

        let (parity, _, _) = curve_path_d_and_bounds(&line_data, "rounded", 12.0, false, None);
        let (compact, _, _) = curve_path_d_and_bounds(&line_data, "rounded", 12.0, true, None);

        assert!(
            parity.starts_with("M130.484,214.303L133.134,222.253Q135.784,230.203 135.784,238.583"),
            "expected the Mermaid-compatible half-segment cut: {parity}"
        );
        assert!(
            compact.starts_with("M130.484,214.303L135.168,228.356Q135.784,230.203 135.784,232.15"),
            "expected the compact tangent-length cut: {compact}"
        );

        let corner_mask = [true, false, true, true];
        let (adapter_linear, _, _) =
            curve_path_d_and_bounds(&line_data, "rounded", 12.0, true, Some(&corner_mask));
        assert!(
            adapter_linear
                .starts_with("M130.484,214.303L135.784,230.203L135.784,240.203Q135.784,250.203"),
            "expected the endpoint adapter to stay linear while the ELK bend remains rounded: {adapter_linear}"
        );
    }
}
