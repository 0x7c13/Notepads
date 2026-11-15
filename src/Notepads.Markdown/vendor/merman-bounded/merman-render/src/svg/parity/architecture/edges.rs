use std::fmt::Write as _;

use crate::architecture_metrics::{
    ARCHITECTURE_CREATE_TEXT_DEFAULT_WRAP_WIDTH_PX, ARCHITECTURE_SERVICE_LABEL_BOTTOM_EXTENSION_PX,
    architecture_create_text_middle_bbox_y_range_px,
};
use crate::model::Bounds;
use crate::text::TextMeasurer;

use super::super::{SvgDiagramId, escape_xml_into, fmt, fmt_into, fmt_string};
use super::ArchitectureEmitCheckpoints;
use super::geometry::{arrow_shift, bounds_from_rect, extend_bounds, is_arch_dir_x, is_arch_dir_y};
use super::labels::{
    svg_line_formatted_bbox_width_px, svg_line_plain_text, wrap_svg_words_to_lines,
    write_svg_text_lines,
};
use super::model::ArchitectureModelAccess;
use super::settings::ArchitectureRenderSettings;
use crate::model::ArchitectureDiagramLayout;

pub(super) struct ArchitectureEdgeRenderContext<'a, M: ArchitectureModelAccess> {
    pub(super) out: &'a mut String,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) layout: &'a ArchitectureDiagramLayout,
    pub(super) model: &'a M,
    pub(super) node_xy: &'a rustc_hash::FxHashMap<&'a str, (f64, f64)>,
    pub(super) settings: &'a ArchitectureRenderSettings,
    pub(super) text_measurer: &'a dyn TextMeasurer,
    pub(super) content_bounds: &'a mut Option<Bounds>,
    pub(super) junction_bounds: &'a rustc_hash::FxHashMap<&'a str, Bounds>,
    pub(super) checkpoints: ArchitectureEmitCheckpoints<'a>,
}

struct ArchitectureEdgeLabelPlan {
    lines: Vec<super::labels::SvgLine>,
    bounds: Bounds,
    dominant_baseline: &'static str,
    transform: String,
}

#[derive(Clone, Copy)]
struct ArchitectureEdgePoints {
    start_x: f64,
    start_y: f64,
    mid_x: f64,
    mid_y: f64,
    end_x: f64,
    end_y: f64,
}

struct ArchitectureArrowPoints {
    left: String,
    right: String,
    top: String,
    bottom: String,
}

impl ArchitectureArrowPoints {
    fn new(arrow_size: f64) -> Self {
        let s = fmt_string(arrow_size);
        let hs = fmt_string(arrow_size / 2.0);
        Self {
            left: format!("{s},{hs} 0,{s} 0,0"),
            right: format!("0,{hs} {s},0 {s},{s}"),
            top: format!("0,0 {s},0 {hs},{s}"),
            bottom: format!("{hs},0 {s},{s} 0,{s}"),
        }
    }

    fn get(&self, dir: char) -> &str {
        match dir {
            'L' => self.left.as_str(),
            'R' => self.right.as_str(),
            'T' => self.top.as_str(),
            'B' => self.bottom.as_str(),
            _ => self.right.as_str(),
        }
    }
}

#[derive(Clone, Copy)]
enum ArchitectureArrowTransform {
    Translate {
        x: f64,
        y: f64,
    },
    TranslateRotate {
        x: f64,
        y: f64,
        angle: f64,
        cx: f64,
        cy: f64,
    },
}

struct ArchitectureArrowGeometry<'a> {
    points: &'a str,
    transform: ArchitectureArrowTransform,
    bounds: Bounds,
}

fn write_architecture_edge_id_attr(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    prefix: &str,
    from: &str,
    to: &str,
    counter: usize,
) {
    let _ = write!(out, "{diagram_id}");
    out.push('-');
    escape_xml_into(out, prefix);
    out.push('_');
    escape_xml_into(out, from);
    out.push('_');
    escape_xml_into(out, to);
    out.push('_');
    let _ = write!(out, "{counter}");
}

fn write_architecture_edge_path(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    edge: super::model::ArchitectureEdgeRef<'_>,
    points: ArchitectureEdgePoints,
) {
    out.push_str(r#"<path d="M "#);
    fmt_into(out, points.start_x);
    out.push(',');
    fmt_into(out, points.start_y);
    out.push_str(" L ");
    fmt_into(out, points.mid_x);
    out.push(',');
    fmt_into(out, points.mid_y);
    out.push_str(" L");
    fmt_into(out, points.end_x);
    out.push(',');
    fmt_into(out, points.end_y);
    out.push_str(r#" " class="edge" id=""#);
    write_architecture_edge_id_attr(out, diagram_id, "L", edge.lhs_id, edge.rhs_id, 0);
    out.push_str(r#""/>"#);
}

fn write_architecture_arrow_transform(out: &mut String, transform: ArchitectureArrowTransform) {
    match transform {
        ArchitectureArrowTransform::Translate { x, y } => {
            out.push_str("translate(");
            fmt_into(out, x);
            out.push(',');
            fmt_into(out, y);
            out.push(')');
        }
        ArchitectureArrowTransform::TranslateRotate {
            x,
            y,
            angle,
            cx,
            cy,
        } => {
            out.push_str("translate(");
            fmt_into(out, x);
            out.push(',');
            fmt_into(out, y);
            out.push_str(") rotate(");
            fmt_into(out, angle);
            out.push(',');
            fmt_into(out, cx);
            out.push(',');
            fmt_into(out, cy);
            out.push(')');
        }
    }
}

fn write_architecture_arrow_polygon(out: &mut String, arrow: &ArchitectureArrowGeometry<'_>) {
    out.push_str(r#"<polygon points=""#);
    out.push_str(arrow.points);
    out.push_str(r#"" transform=""#);
    write_architecture_arrow_transform(out, arrow.transform);
    out.push_str(r#"" class="arrow"/>"#);
}

fn architecture_dir_unit(dir: char) -> (f64, f64) {
    match dir {
        'L' => (1.0, 0.0),
        'R' => (-1.0, 0.0),
        'T' => (0.0, 1.0),
        'B' => (0.0, -1.0),
        _ => (1.0, 0.0),
    }
}

fn architecture_arrow_geometry<'a>(
    dir: char,
    anchor_x: f64,
    anchor_y: f64,
    adjacent_x: f64,
    adjacent_y: f64,
    arrow_size: f64,
    arrow_points: &'a ArchitectureArrowPoints,
) -> ArchitectureArrowGeometry<'a> {
    let half_arrow_size = arrow_size / 2.0;
    let dx = anchor_x - adjacent_x;
    let dy = anchor_y - adjacent_y;
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > 1e-6 {
        (dx / len, dy / len)
    } else {
        architecture_dir_unit(dir)
    };

    let port_x_shift = if is_arch_dir_x(dir) {
        arrow_shift(dir, anchor_x, arrow_size)
    } else {
        anchor_x - half_arrow_size
    };
    let port_y_shift = if is_arch_dir_y(dir) {
        arrow_shift(dir, anchor_y, arrow_size)
    } else {
        anchor_y - half_arrow_size
    };

    if ux.abs() < 1e-6 || uy.abs() < 1e-6 {
        return ArchitectureArrowGeometry {
            points: arrow_points.get(dir),
            transform: ArchitectureArrowTransform::Translate {
                x: port_x_shift,
                y: port_y_shift,
            },
            bounds: bounds_from_rect(port_x_shift, port_y_shift, arrow_size, arrow_size),
        };
    }

    // Mermaid positions Architecture arrows as standalone polygons rather than SVG markers.
    // Rotate that polygon by the actual final segment, so diagonal crosslinks point along the
    // edge instead of only following the requested port side.
    let tip_x = anchor_x + 2.0 * ux;
    let tip_y = anchor_y + 2.0 * uy;
    let base_x = tip_x - arrow_size * ux;
    let base_y = tip_y - arrow_size * uy;
    let perp_x = -uy * half_arrow_size;
    let perp_y = ux * half_arrow_size;
    let p0 = (base_x + perp_x, base_y + perp_y);
    let p1 = (base_x - perp_x, base_y - perp_y);
    let p2 = (tip_x, tip_y);
    let exact_bounds = Bounds {
        min_x: p0.0.min(p1.0).min(p2.0),
        min_y: p0.1.min(p1.1).min(p2.1),
        max_x: p0.0.max(p1.0).max(p2.0),
        max_y: p0.1.max(p1.1).max(p2.1),
    };
    let port_bounds = bounds_from_rect(port_x_shift, port_y_shift, arrow_size, arrow_size);
    let angle = (-ux).atan2(uy).to_degrees();

    ArchitectureArrowGeometry {
        points: arrow_points.get('T'),
        transform: ArchitectureArrowTransform::TranslateRotate {
            x: tip_x - half_arrow_size,
            y: tip_y - arrow_size,
            angle,
            cx: half_arrow_size,
            cy: arrow_size,
        },
        bounds: Bounds {
            min_x: exact_bounds.min_x.min(port_bounds.min_x),
            min_y: exact_bounds.min_y.min(port_bounds.min_y),
            max_x: exact_bounds.max_x.max(port_bounds.max_x),
            max_y: exact_bounds.max_y.max(port_bounds.max_y),
        },
    }
}

fn architecture_edge_segment_bounds(points: ArchitectureEdgePoints) -> Bounds {
    Bounds {
        min_x: points.start_x.min(points.mid_x).min(points.end_x),
        min_y: points.start_y.min(points.mid_y).min(points.end_y),
        max_x: points.start_x.max(points.mid_x).max(points.end_x),
        max_y: points.start_y.max(points.mid_y).max(points.end_y),
    }
}

fn architecture_edge_label_plan(
    edge: super::model::ArchitectureEdgeRef<'_>,
    points: ArchitectureEdgePoints,
    settings: &ArchitectureRenderSettings,
    text_measurer: &dyn TextMeasurer,
) -> Option<ArchitectureEdgeLabelPlan> {
    let label = edge.title.map(str::trim).filter(|t| !t.is_empty())?;
    let axis = match (is_arch_dir_x(edge.lhs_dir), is_arch_dir_x(edge.rhs_dir)) {
        (true, true) => "X",
        (false, false) => "Y",
        _ => "XY",
    };

    let wrap_width = match axis {
        "X" => (points.start_x - points.end_x).abs(),
        "Y" => (points.start_y - points.end_y).abs() / 1.5,
        _ => (points.start_x - points.end_x).abs() / 2.0,
    };
    let wrap_width = if wrap_width.is_finite() && wrap_width > 0.0 {
        wrap_width
    } else {
        ARCHITECTURE_CREATE_TEXT_DEFAULT_WRAP_WIDTH_PX
    };
    let lines = wrap_svg_words_to_lines(label, wrap_width, text_measurer, &settings.text_style);

    let mut bbox_w = 0.0f64;
    let mut first_line_text = None;
    for line in &lines {
        let s = svg_line_plain_text(line);
        bbox_w = bbox_w.max(svg_line_formatted_bbox_width_px(
            line,
            text_measurer,
            &settings.text_style,
        ));
        first_line_text.get_or_insert(s);
    }
    let line_count = lines.len().max(1);
    let (bbox_y_min, bbox_y_max) = architecture_create_text_middle_bbox_y_range_px(
        first_line_text.as_deref().unwrap_or(label),
        &settings.text_style,
        line_count,
        text_measurer,
    );
    let bbox_h = bbox_y_max - bbox_y_min;
    let half_bbox_h = bbox_h / 2.0;
    let half_bbox_w = bbox_w / 2.0;

    let (dominant_baseline, transform) = match axis {
        "Y" => (
            "middle",
            format!(
                r#"translate({}, {}) rotate(-90)"#,
                fmt(points.mid_x),
                fmt(points.mid_y)
            ),
        ),
        "XY" => {
            let pair = format!("{}{}", edge.lhs_dir, edge.rhs_dir);
            let (xf, yf): (f64, f64) = match pair.as_str() {
                "LT" | "TL" => (1.0, 1.0),
                "BL" | "LB" => (1.0, -1.0),
                "BR" | "RB" => (-1.0, -1.0),
                _ => (-1.0, 1.0),
            };
            let angle = (-xf * yf * 45.0f64).round() as i64;

            // Rotated bbox at 45° (w' == h' == (w+h)*sqrt(2)/2).
            let diag = (bbox_w + bbox_h) * std::f64::consts::FRAC_1_SQRT_2;
            let t2x = xf * diag / 2.0;
            let t2y = yf * diag / 2.0;
            // Mermaid CLI serializes newline characters inside attribute values as XML entities
            // (`&#10;`). Emit those explicitly so our SVG matches the upstream baselines.
            let sep = "&#10;";

            (
                "auto",
                format!(
                    "translate({}, {}){sep}                translate({}, {}){sep}                rotate({}, 0, {})",
                    fmt(points.mid_x),
                    fmt(points.mid_y - half_bbox_h),
                    fmt(t2x),
                    fmt(t2y),
                    angle,
                    fmt(half_bbox_h),
                    sep = sep
                ),
            )
        }
        _ => (
            "middle",
            format!(r#"translate({}, {})"#, fmt(points.mid_x), fmt(points.mid_y)),
        ),
    };

    let bounds = match axis {
        "X" => Bounds {
            min_x: points.mid_x - half_bbox_w,
            min_y: points.mid_y + bbox_y_min,
            max_x: points.mid_x + half_bbox_w,
            max_y: points.mid_y + bbox_y_max,
        },
        "Y" => Bounds {
            min_x: points.mid_x + bbox_y_min,
            min_y: points.mid_y - half_bbox_w,
            max_x: points.mid_x + bbox_y_max,
            max_y: points.mid_y + half_bbox_w,
        },
        _ => {
            // |cos(45°)| == |sin(45°)| == sqrt(1/2)
            let a = (bbox_w + bbox_h) * std::f64::consts::FRAC_1_SQRT_2;
            Bounds {
                min_x: points.mid_x - a / 2.0,
                min_y: points.mid_y - a / 2.0,
                max_x: points.mid_x + a / 2.0,
                max_y: points.mid_y + a / 2.0,
            }
        }
    };

    Some(ArchitectureEdgeLabelPlan {
        lines,
        bounds,
        dominant_baseline,
        transform,
    })
}

pub(super) fn push_architecture_edges<M: ArchitectureModelAccess>(
    ctx: &mut ArchitectureEdgeRenderContext<'_, M>,
) -> crate::Result<()> {
    let checkpoints = ctx.checkpoints;
    checkpoints.checkpoint()?;
    let out = &mut *ctx.out;
    let diagram_id = ctx.diagram_id;
    let layout = ctx.layout;
    let model = ctx.model;
    let node_xy = ctx.node_xy;
    let settings = ctx.settings;
    let text_measurer = ctx.text_measurer;
    let content_bounds = &mut *ctx.content_bounds;
    let junction_bounds = ctx.junction_bounds;

    let group_edge_shift = settings.padding_px + 4.0;
    let group_edge_label_bottom_px = ARCHITECTURE_SERVICE_LABEL_BOTTOM_EXTENSION_PX;
    let is_junction = |id: &str| junction_bounds.contains_key(id);

    let layout_edge_points: Vec<(f64, f64, f64, f64, f64, f64)> = layout
        .edges
        .iter()
        .map(|e| {
            // Architecture layout edges are expected to be 3-point polylines.
            // Be defensive and fall back to zeros if the snapshot is malformed.
            let p0 = e.points.first().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
            let pm = e.points.get(1).map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
            let p2 = e.points.last().map(|p| (p.x, p.y)).unwrap_or((0.0, 0.0));
            (p0.0, p0.1, pm.0, pm.1, p2.0, p2.1)
        })
        .collect();

    let edge_points =
        |edge_idx: usize, edge: super::model::ArchitectureEdgeRef<'_>| -> ArchitectureEdgePoints {
            // Prefer layout-provided points: this is where we model Mermaid/Cytoscape edge routing.
            //
            // The layout points represent raw Cytoscape endpoints; Mermaid applies group/junction
            // endpoint shifts later, during SVG emission.
            let (raw_start_x, raw_start_y, mid_x, mid_y, raw_end_x, raw_end_y) = layout_edge_points
                .get(edge_idx)
                .copied()
                .unwrap_or_else(|| {
                    let (sx, sy) = node_xy.get(edge.lhs_id).copied().unwrap_or((0.0, 0.0));
                    let (tx, ty) = node_xy.get(edge.rhs_id).copied().unwrap_or((0.0, 0.0));

                    let (sx, sy) = match edge.lhs_dir {
                        'L' => (sx, sy + settings.half_icon),
                        'R' => (sx + settings.icon_size_px, sy + settings.half_icon),
                        'T' => (sx + settings.half_icon, sy),
                        'B' => (sx + settings.half_icon, sy + settings.icon_size_px),
                        _ => (sx + settings.half_icon, sy + settings.half_icon),
                    };
                    let (tx, ty) = match edge.rhs_dir {
                        'L' => (tx, ty + settings.half_icon),
                        'R' => (tx + settings.icon_size_px, ty + settings.half_icon),
                        'T' => (tx + settings.half_icon, ty),
                        'B' => (tx + settings.half_icon, ty + settings.icon_size_px),
                        _ => (tx + settings.half_icon, ty + settings.half_icon),
                    };

                    let (mx, my) = if (sx - tx).abs() > 1e-6 && (sy - ty).abs() > 1e-6 {
                        // Match upstream Mermaid: choose the bend based on the *source* dir.
                        if is_arch_dir_y(edge.lhs_dir) {
                            (sx, ty)
                        } else {
                            (tx, sy)
                        }
                    } else {
                        ((sx + tx) / 2.0, (sy + ty) / 2.0)
                    };
                    (sx, sy, mx, my, tx, ty)
                });

            let mut start_x = raw_start_x;
            let mut start_y = raw_start_y;
            let mut end_x = raw_end_x;
            let mut end_y = raw_end_y;

            let lhs_group = edge.lhs_group.unwrap_or(false);
            if lhs_group {
                if is_arch_dir_x(edge.lhs_dir) {
                    start_x += if edge.lhs_dir == 'L' {
                        -group_edge_shift
                    } else {
                        group_edge_shift
                    };
                } else {
                    start_y += if edge.lhs_dir == 'T' {
                        -group_edge_shift
                    } else {
                        group_edge_shift + group_edge_label_bottom_px
                    };
                }
            }
            if !lhs_group && is_junction(edge.lhs_id) {
                if is_arch_dir_x(edge.lhs_dir) {
                    start_x += if edge.lhs_dir == 'L' {
                        settings.half_icon
                    } else {
                        -settings.half_icon
                    };
                } else {
                    start_y += if edge.lhs_dir == 'T' {
                        settings.half_icon
                    } else {
                        -settings.half_icon
                    };
                }
            }

            let rhs_group = edge.rhs_group.unwrap_or(false);
            if rhs_group {
                if is_arch_dir_x(edge.rhs_dir) {
                    end_x += if edge.rhs_dir == 'L' {
                        -group_edge_shift
                    } else {
                        group_edge_shift
                    };
                } else {
                    end_y += if edge.rhs_dir == 'T' {
                        -group_edge_shift
                    } else {
                        group_edge_shift + group_edge_label_bottom_px
                    };
                }
            }
            if !rhs_group && is_junction(edge.rhs_id) {
                if is_arch_dir_x(edge.rhs_dir) {
                    end_x += if edge.rhs_dir == 'L' {
                        settings.half_icon
                    } else {
                        -settings.half_icon
                    };
                } else {
                    end_y += if edge.rhs_dir == 'T' {
                        settings.half_icon
                    } else {
                        -settings.half_icon
                    };
                }
            }

            ArchitectureEdgePoints {
                start_x,
                start_y,
                mid_x,
                mid_y,
                end_x,
                end_y,
            }
        };

    // Edges (including conservative label bounds).
    if model.edges_len() != 0 {
        let arrow_size = settings.icon_size_px / 6.0;
        let arrow_points = ArchitectureArrowPoints::new(arrow_size);
        for (edge_idx, edge) in model.edges().enumerate() {
            let points = edge_points(edge_idx, edge);
            let lhs_arrow = (edge.lhs_into == Some(true)).then(|| {
                architecture_arrow_geometry(
                    edge.lhs_dir,
                    points.start_x,
                    points.start_y,
                    points.mid_x,
                    points.mid_y,
                    arrow_size,
                    &arrow_points,
                )
            });
            let rhs_arrow = (edge.rhs_into == Some(true)).then(|| {
                architecture_arrow_geometry(
                    edge.rhs_dir,
                    points.end_x,
                    points.end_y,
                    points.mid_x,
                    points.mid_y,
                    arrow_size,
                    &arrow_points,
                )
            });

            extend_bounds(content_bounds, architecture_edge_segment_bounds(points));

            if let Some(arrow) = lhs_arrow.as_ref() {
                extend_bounds(content_bounds, arrow.bounds.clone());
            }

            if let Some(arrow) = rhs_arrow.as_ref() {
                extend_bounds(content_bounds, arrow.bounds.clone());
            }

            let label_plan = architecture_edge_label_plan(edge, points, settings, text_measurer);
            if let Some(label_plan) = label_plan.as_ref() {
                extend_bounds(content_bounds, label_plan.bounds.clone());
            }

            out.push_str("<g>");
            write_architecture_edge_path(out, diagram_id, edge, points);
            checkpoints.checkpoint()?;

            if let Some(arrow) = lhs_arrow.as_ref() {
                write_architecture_arrow_polygon(out, arrow);
            }

            if let Some(arrow) = rhs_arrow.as_ref() {
                write_architecture_arrow_polygon(out, arrow);
            }

            if let Some(label_plan) = label_plan {
                let _ = write!(
                    out,
                    r#"<g dy="1em" alignment-baseline="middle" dominant-baseline="{baseline}" text-anchor="middle" transform="{transform}">"#,
                    baseline = label_plan.dominant_baseline,
                    transform = label_plan.transform.as_str()
                );
                out.push_str(r#"<g><rect class="background" style="stroke: none"/>"#);
                write_svg_text_lines(out, &label_plan.lines);
                out.push_str("</g></g>");
            }

            out.push_str("</g>");
        }
    }
    Ok(())
}
