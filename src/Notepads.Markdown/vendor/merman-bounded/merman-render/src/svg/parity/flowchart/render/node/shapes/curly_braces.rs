//! Flowchart v2 curly brace / comment shapes.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

use super::super::geom::path_from_points;
use super::super::roughjs::roughjs_stroke_path_for_svg_path;

pub(in crate::svg::parity::flowchart) struct CurlyBraceCommentGeometry {
    pub(in crate::svg::parity::flowchart) group_tx: f64,
    pub(in crate::svg::parity::flowchart) label_dx: f64,
    pub(in crate::svg::parity::flowchart) label_dy: f64,
    pub(in crate::svg::parity::flowchart) paths: Vec<CurlyBraceCommentPath>,
}

pub(in crate::svg::parity::flowchart) struct CurlyBraceCommentPath {
    pub(in crate::svg::parity::flowchart) d: String,
    pub(in crate::svg::parity::flowchart) visible: bool,
}

fn circle_points(
    center_x: f64,
    center_y: f64,
    radius: f64,
    num_points: usize,
    start_deg: f64,
    end_deg: f64,
    negate: bool,
) -> Vec<(f64, f64)> {
    let start = start_deg.to_radians();
    let end = end_deg.to_radians();
    let angle_range = end - start;
    let angle_step = if num_points > 1 {
        angle_range / (num_points as f64 - 1.0)
    } else {
        0.0
    };
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(num_points);
    for i in 0..num_points {
        let a = start + (i as f64) * angle_step;
        let x = center_x + radius * a.cos();
        let y = center_y + radius * a.sin();
        if negate {
            out.push((-x, -y));
        } else {
            out.push((x, y));
        }
    }
    out
}

pub(in crate::svg::parity::flowchart) fn curly_brace_comment_intersection_points(
    shape: &str,
    label_w: f64,
    label_h: f64,
    padding: f64,
    look_is_neo: bool,
) -> Vec<(f64, f64)> {
    let (w, h) = crate::flowchart::flowchart_brace_content_dimensions(
        shape,
        label_w,
        label_h,
        padding,
        look_is_neo,
    );
    let radius = (h * 0.1).max(5.0);
    // Keep the pinned rectPoints separate from the visible brace paths: their
    // central indentation and sampling differ, and intersections need no SVG.
    if shape == "braces" {
        let rect_points: Vec<(f64, f64)> = [
            vec![(w / 2.0, -h / 2.0 - radius), (-w / 2.0, -h / 2.0 - radius)],
            circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0, true),
            vec![(-w / 2.0 - radius, -radius)],
            circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                true,
            ),
            circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ),
            vec![(-w / 2.0 - radius, h / 2.0)],
            circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true),
            vec![
                (-w / 2.0, h / 2.0 + radius),
                (w / 2.0 - radius - radius / 2.0, h / 2.0 + radius),
            ],
            circle_points(
                -w / 2.0 + radius + radius / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ),
            vec![(w / 2.0 - radius / 2.0, radius)],
            circle_points(
                -w / 2.0 - radius / 2.0,
                -radius,
                radius,
                20,
                0.0,
                90.0,
                true,
            ),
            circle_points(
                -w / 2.0 - radius / 2.0,
                radius,
                radius,
                20,
                -90.0,
                0.0,
                true,
            ),
            vec![(w / 2.0 - radius / 2.0, -radius)],
            circle_points(
                -w / 2.0 + radius + radius / 2.0,
                h / 2.0,
                radius,
                30,
                -180.0,
                -270.0,
                true,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        rect_points
    } else if shape == "brace-r" {
        let rect_points: Vec<(f64, f64)> = [
            vec![(-w / 2.0, -h / 2.0 - radius), (w / 2.0, -h / 2.0 - radius)],
            circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0, false),
            vec![(w / 2.0 + radius, -radius)],
            circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                false,
            ),
            circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                false,
            ),
            vec![(w / 2.0 + radius, h / 2.0)],
            circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, false),
            vec![(w / 2.0, h / 2.0 + radius), (-w / 2.0, h / 2.0 + radius)],
        ]
        .into_iter()
        .flatten()
        .collect();
        rect_points
    } else {
        let rect_points: Vec<(f64, f64)> = [
            vec![(w / 2.0, -h / 2.0 - radius), (-w / 2.0, -h / 2.0 - radius)],
            circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0, true),
            vec![(-w / 2.0 - radius, -radius)],
            circle_points(w / 2.0 + w * 0.1, -radius, radius, 20, -180.0, -270.0, true),
            circle_points(w / 2.0 + w * 0.1, radius, radius, 20, -90.0, -180.0, true),
            vec![(-w / 2.0 - radius, h / 2.0)],
            circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true),
            vec![(-w / 2.0, h / 2.0 + radius), (w / 2.0, h / 2.0 + radius)],
        ]
        .into_iter()
        .flatten()
        .collect();
        rect_points
    }
}

pub(in crate::svg::parity::flowchart) fn curly_brace_comment_geometry(
    shape: &str,
    label_w: f64,
    label_h: f64,
    padding: f64,
    look_is_neo: bool,
) -> CurlyBraceCommentGeometry {
    let padding = padding.max(0.0);
    let (w, h) = crate::flowchart::flowchart_brace_content_dimensions(
        shape,
        label_w,
        label_h,
        padding,
        look_is_neo,
    );
    let radius = (h * 0.1).max(5.0);

    // The source label transforms use the original node padding, even when
    // Neo chooses different shape padding. Express them relative to the
    // shared label renderer's centered label box.
    let label_padding_dx = (label_w - w + padding) / 2.0;
    let label_dy = (label_h - h + padding) / 2.0;
    let (group_tx, label_dx) = match shape {
        "comment" | "brace" | "brace-l" => (radius, (label_w - w) / 2.0 + radius),
        "brace-r" => (-radius, label_padding_dx),
        "braces" => (radius - radius / 4.0, label_padding_dx),
        _ => (0.0, 0.0),
    };

    let rect_points =
        curly_brace_comment_intersection_points(shape, label_w, label_h, padding, look_is_neo);
    let paths = if shape == "braces" {
        // Mermaid `curlyBraces.ts`: two visible brace paths + one invisible rect path.
        let left_points: Vec<(f64, f64)> = [
            circle_points(w / 2.0, -h / 2.0, radius, 30, -90.0, 0.0, true),
            vec![(-w / 2.0 - radius, radius)],
            circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                true,
            ),
            circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ),
            vec![(-w / 2.0 - radius, -h / 2.0)],
            circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true),
        ]
        .into_iter()
        .flatten()
        .collect();
        let right_points: Vec<(f64, f64)> = [
            circle_points(
                -w / 2.0 + radius + radius / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ),
            vec![(w / 2.0 - radius / 2.0, radius)],
            circle_points(
                -w / 2.0 - radius / 2.0,
                -radius,
                radius,
                20,
                0.0,
                90.0,
                true,
            ),
            circle_points(
                -w / 2.0 - radius / 2.0,
                radius,
                radius,
                20,
                -90.0,
                0.0,
                true,
            ),
            vec![(w / 2.0 - radius / 2.0, -radius)],
            circle_points(
                -w / 2.0 + radius + radius / 2.0,
                h / 2.0,
                radius,
                30,
                -180.0,
                -270.0,
                true,
            ),
        ]
        .into_iter()
        .flatten()
        .collect();

        let left_path = path_from_points(&left_points)
            .trim_end_matches('Z')
            .to_string();
        let right_path = path_from_points(&right_points)
            .trim_end_matches('Z')
            .to_string();
        let rect_path = path_from_points(&rect_points);
        // Upstream inserts each path at :first-child: right, left, then the hidden rectangle.
        vec![
            CurlyBraceCommentPath {
                d: right_path,
                visible: true,
            },
            CurlyBraceCommentPath {
                d: left_path,
                visible: true,
            },
            CurlyBraceCommentPath {
                d: rect_path,
                visible: false,
            },
        ]
    } else {
        // Mermaid `curlyBraceLeft.ts` / `curlyBraceRight.ts`.
        let points = if shape == "brace-r" {
            let points: Vec<(f64, f64)> = [
                circle_points(w / 2.0, -h / 2.0, radius, 20, -90.0, 0.0, false),
                vec![(w / 2.0 + radius, -radius)],
                circle_points(
                    w / 2.0 + radius * 2.0,
                    -radius,
                    radius,
                    20,
                    -180.0,
                    -270.0,
                    false,
                ),
                circle_points(
                    w / 2.0 + radius * 2.0,
                    radius,
                    radius,
                    20,
                    -90.0,
                    -180.0,
                    false,
                ),
                vec![(w / 2.0 + radius, h / 2.0)],
                circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, false),
            ]
            .into_iter()
            .flatten()
            .collect();

            points
        } else {
            let points: Vec<(f64, f64)> = [
                circle_points(w / 2.0, -h / 2.0, radius, 30, -90.0, 0.0, true),
                vec![(-w / 2.0 - radius, radius)],
                circle_points(
                    w / 2.0 + radius * 2.0,
                    -radius,
                    radius,
                    20,
                    -180.0,
                    -270.0,
                    true,
                ),
                circle_points(
                    w / 2.0 + radius * 2.0,
                    radius,
                    radius,
                    20,
                    -90.0,
                    -180.0,
                    true,
                ),
                vec![(-w / 2.0 - radius, -h / 2.0)],
                circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true),
            ]
            .into_iter()
            .flatten()
            .collect();

            points
        };

        let brace_path = path_from_points(&points).trim_end_matches('Z').to_string();
        let rect_path = path_from_points(&rect_points);
        vec![
            CurlyBraceCommentPath {
                d: brace_path,
                visible: true,
            },
            CurlyBraceCommentPath {
                d: rect_path,
                visible: false,
            },
        ]
    };

    CurlyBraceCommentGeometry {
        group_tx,
        label_dx,
        label_dy,
        paths,
    }
}

pub(in crate::svg::parity::flowchart::render::node) fn render_curly_brace_comment(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let metrics = super::super::helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    let geometry = curly_brace_comment_geometry(
        common.shape,
        metrics.width,
        metrics.height,
        ctx.node_padding,
        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
    );
    label.dx = geometry.label_dx;
    label.dy = geometry.label_dy;

    let mut stroke_d = |d: &str| {
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_stroke_path_for_svg_path(
                d,
                common.stroke_width,
                common.stroke_dasharray,
                common.hand_drawn_seed,
            )
        })
        .unwrap_or_else(|| "M0,0".to_string())
    };

    let _ = write!(
        out,
        r##"<g class="text" transform="translate({},0)">"##,
        fmt(geometry.group_tx),
    );
    for path in geometry.paths {
        let d = stroke_d(&path.d);
        if path.visible {
            out.push_str("<g>");
        } else {
            out.push_str(r#"<g stroke-opacity="0">"#);
        }
        let _ = write!(
            out,
            r##"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"##,
            escape_attr(&d),
            escape_attr(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_attr(common.stroke_dasharray),
            escape_attr(common.style),
        );
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn union_translated_path_bounds(
        geometry: &CurlyBraceCommentGeometry,
    ) -> crate::svg::parity::path_bounds::SvgPathBounds {
        let mut out: Option<crate::svg::parity::path_bounds::SvgPathBounds> = None;
        for path in &geometry.paths {
            let mut pb = crate::svg::parity::path_bounds::svg_path_bounds_from_d(&path.d)
                .expect("path bounds");
            pb.min_x += geometry.group_tx;
            pb.max_x += geometry.group_tx;
            out = Some(match out {
                Some(mut acc) => {
                    acc.min_x = acc.min_x.min(pb.min_x);
                    acc.min_y = acc.min_y.min(pb.min_y);
                    acc.max_x = acc.max_x.max(pb.max_x);
                    acc.max_y = acc.max_y.max(pb.max_y);
                    acc
                }
                None => pb,
            });
        }
        out.expect("non-empty geometry")
    }

    #[test]
    fn brace_geometry_matches_layout_and_source_label_transforms() {
        for (shape, neo_dx, neo_dy) in [
            ("brace", -4.0, 1.5),
            ("brace-l", -4.0, 1.5),
            ("comment", -4.0, 1.5),
            ("brace-r", -10.5, -4.5),
            ("braces", -10.5, -4.5),
        ] {
            let geometry = curly_brace_comment_geometry(shape, 100.0, 20.0, 15.0, true);
            assert_eq!(geometry.label_dx, neo_dx, "{shape}");
            assert_eq!(geometry.label_dy, neo_dy, "{shape}");
            let bounds = union_translated_path_bounds(&geometry);
            let layout = crate::flowchart::flowchart_node_render_dimensions(
                Some(shape),
                crate::text::TextMetrics {
                    width: 100.0,
                    height: 20.0,
                    line_count: 1,
                },
                15.0,
                true,
            );
            assert!(
                (bounds.max_x - bounds.min_x - layout.0).abs() < 1e-9,
                "{shape}"
            );
            assert!(
                (bounds.max_y - bounds.min_y - layout.1).abs() < 1e-9,
                "{shape}"
            );
            for padding in [0.0, -2.0] {
                let empty = curly_brace_comment_geometry(shape, 0.0, 0.0, padding, false);
                let bounds = union_translated_path_bounds(&empty);
                let width = match shape {
                    "brace-r" => 10.0,
                    "braces" => 12.5,
                    _ => 15.0,
                };
                assert!(
                    (bounds.max_x - bounds.min_x - width).abs() < 1e-9,
                    "{shape}"
                );
                assert!((bounds.max_y - bounds.min_y - 10.0).abs() < 1e-9, "{shape}");
                assert_eq!(empty.label_dy, 0.0, "{shape}");
            }
        }
        // A tall classic label increases radius beyond the default 5px.
        // Left-brace text follows r - padding/2, not the historical -r/2.
        let tall = curly_brace_comment_geometry("brace", 100.0, 85.0, 15.0, false);
        assert_eq!(tall.label_dx, 2.5);
        assert_eq!(tall.label_dy, 0.0);
    }

    #[test]
    fn paired_braces_follow_source_dom_insertion_order() {
        let geometry = curly_brace_comment_geometry("braces", 100.0, 20.0, 15.0, true);
        assert_eq!(geometry.paths.len(), 3);
        let bounds = |index: usize| {
            crate::svg::parity::path_bounds::svg_path_bounds_from_d(&geometry.paths[index].d)
                .expect("path bounds")
        };
        assert!(geometry.paths[0].visible);
        assert!(geometry.paths[1].visible);
        assert!(!geometry.paths[2].visible);
        assert!(
            bounds(0).min_x > 0.0,
            "the right brace is inserted last and painted first"
        );
        assert!(
            bounds(1).max_x < 0.0,
            "the left brace follows the right brace"
        );
    }

    #[test]
    fn brace_r_geometry_uses_label_box_not_updated_layout_box() {
        let label_w = 198.320_312_5;
        let label_h = 54.2;
        let padding = 15.0;
        let geometry = curly_brace_comment_geometry("brace-r", label_w, label_h, padding, false);

        assert_eq!(geometry.paths.len(), 2);
        assert!(geometry.paths[0].visible);
        assert!(!geometry.paths[1].visible);
        assert!((geometry.group_tx + 6.92).abs() < 1e-9);

        let bounds = union_translated_path_bounds(&geometry);
        let expected_width = label_w + padding + 2.0 * 6.92;
        assert!(((bounds.max_x - bounds.min_x) - expected_width).abs() < 1e-9);
    }
}
