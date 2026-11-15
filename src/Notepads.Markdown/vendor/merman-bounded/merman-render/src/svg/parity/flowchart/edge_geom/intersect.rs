use super::super::*;
use super::BoundaryNode;

pub(in crate::svg::parity::flowchart) fn is_rounded_intersect_shift_shape(
    layout_shape: Option<&str>,
) -> bool {
    matches!(layout_shape, Some("roundedRect" | "rounded" | "event"))
}

pub(in crate::svg::parity::flowchart) fn is_polygon_layout_shape(
    layout_shape: Option<&str>,
) -> bool {
    matches!(
        layout_shape,
        Some(
            "hexagon"
                | "hex"
                | "prepare"
                | "odd"
                | "rect_left_inv_arrow"
                | "stadium"
                | "terminal"
                | "pill"
                | "subroutine"
                | "fr-rect"
                | "subproc"
                | "subprocess"
                | "framed-rectangle"
                | "lean_right"
                | "lean-r"
                | "lean-right"
                | "in-out"
                | "lean_left"
                | "lean-l"
                | "lean-left"
                | "out-in"
                | "trapezoid"
                | "trap-b"
                | "priority"
                | "trapezoid-bottom"
                | "inv_trapezoid"
                | "inv-trapezoid"
                | "trap-t"
                | "manual"
                | "trapezoid-top"
                | "curv-trap"
                | "display"
                | "curved-trapezoid"
                | "tri"
                | "extract"
                | "triangle"
                | "manual-file"
                | "flipped-triangle"
                | "flip-tri"
                | "manual-input"
                | "sloped-rectangle"
                | "sl-rect"
                | "hourglass"
                | "collate"
                | "bucket"
                | "folder"
                | "directory"
                | "person"
        )
    )
}

pub(in crate::svg::parity::flowchart) fn force_intersect_for_layout_shape(
    layout_shape: Option<&str>,
) -> bool {
    matches!(
        layout_shape,
        Some(
            "anchor"
                | "sm-circ"
                | "small-circle"
                | "start"
                | "fr-circ"
                | "framed-circle"
                | "stop"
                | "choice"
                | "f-circ"
                | "junction"
                | "filled-circle"
                | "brace"
                | "brace-l"
                | "comment"
                | "brace-r"
                | "braces"
                | "circle"
                | "circ"
                | "doublecircle"
                | "dbl-circ"
                | "double-circle"
                | "diamond"
                | "diam"
                | "question"
                | "decision"
                | "roundedRect"
                | "rounded"
                | "cylinder"
                | "cyl"
                | "db"
                | "database"
                | "lin-cyl"
                | "disk"
                | "lined-cylinder"
                | "h-cyl"
                | "das"
                | "horizontal-cylinder"
                | "cross-circ"
                | "summary"
                | "crossed-circle"
                | "bow-rect"
                | "stored-data"
                | "bow-tie-rectangle"
                | "tag-rect"
                | "tagged-rectangle"
                | "tag-proc"
                | "tagged-process"
                | "paper-tape"
                | "flag"
                | "notch-rect"
                | "notched-rectangle"
                | "card"
                | "st-rect"
                | "procs"
                | "processes"
                | "stacked-rectangle"
                | "doc"
                | "document"
                | "lin-doc"
                | "lined-document"
                | "delay"
                | "half-rounded-rectangle"
                | "notch-pent"
                | "loop-limit"
                | "notched-pentagon"
                | "docs"
                | "documents"
                | "st-doc"
                | "stacked-document"
                | "tag-doc"
                | "tagged-document"
                | "bolt"
                | "com-link"
                | "lightning-bolt"
                | "win-pane"
                | "internal-storage"
                | "window-pane"
                | "stadium"
                | "terminal"
                | "pill",
        )
    ) || is_polygon_layout_shape(layout_shape)
}

fn intersect_rect(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let x = node.x;
    let y = node.y;
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = node.width / 2.0;
    let mut h = node.height / 2.0;

    let (sx, sy) = if dy.abs() * w > dx.abs() * h {
        if dy < 0.0 {
            h = -h;
        }
        let sx = if dy == 0.0 { 0.0 } else { (h * dx) / dy };
        (sx, h)
    } else {
        if dx < 0.0 {
            w = -w;
        }
        let sy = if dx == 0.0 { 0.0 } else { (w * dy) / dx };
        (w, sy)
    };

    crate::model::LayoutPoint {
        x: x + sx,
        y: y + sy,
    }
}

fn intersect_circle(
    node: &BoundaryNode,
    radius: f64,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    // Mermaid's intersect.circle retains the radius captured while painting the shape.
    // The browser's measured bounds can differ, especially for rounded or rough paths.
    let rx = radius;
    let ry = radius;
    let px = node.x - point.x;
    let py = node.y - point.y;
    let det = (rx * rx * py * py + ry * ry * px * px).sqrt();
    // Preserve the source's NaN at the centre: outlineAttachPoint uses that result
    // to decline the departure-axis search and fall back to the centre ray.
    let mut dx = ((rx * ry * px) / det).abs();
    if point.x < node.x {
        dx = -dx;
    }
    let mut dy = ((rx * ry * py) / det).abs();
    if point.y < node.y {
        dy = -dy;
    }
    crate::model::LayoutPoint {
        x: node.x + dx,
        y: node.y + dy,
    }
}

fn intersect_diamond(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let vx = point.x - node.x;
    let vy = point.y - node.y;
    if !(vx.is_finite() && vy.is_finite()) {
        return crate::model::LayoutPoint {
            x: node.x,
            y: node.y,
        };
    }
    if vx.abs() <= 1e-12 && vy.abs() <= 1e-12 {
        return crate::model::LayoutPoint {
            x: node.x,
            y: node.y,
        };
    }
    let hw = (node.width / 2.0).max(1e-9);
    let hh = (node.height / 2.0).max(1e-9);
    let denom = vx.abs() / hw + vy.abs() / hh;
    if !(denom.is_finite() && denom > 0.0) {
        return crate::model::LayoutPoint {
            x: node.x,
            y: node.y,
        };
    }
    let t = 1.0 / denom;
    crate::model::LayoutPoint {
        x: node.x + vx * t,
        y: node.y + vy * t,
    }
}

fn intersect_cylinder(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    // Port of Mermaid `cylinder.ts` intersection logic (11.12.2):
    // - start from `intersect.rect(node, point)`,
    // - then adjust y when the intersection hits the curved top/bottom ellipses.
    let mut pos = intersect_rect(node, point);
    let x = pos.x - node.x;

    let w = node.width.max(1.0);
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);

    if rx != 0.0
        && (x.abs() < w / 2.0
            || ((x.abs() - w / 2.0).abs() < 1e-12
                && (pos.y - node.y).abs() > node.height / 2.0 - ry))
    {
        let mut y = ry * ry * (1.0 - (x * x) / (rx * rx));
        if y > 0.0 {
            y = y.sqrt();
        } else {
            y = 0.0;
        }
        y = ry - y;
        if point.y - node.y > 0.0 {
            y = -y;
        }
        pos.y += y;
    }

    pos
}

fn intersect_tilted_cylinder(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    // Port of Mermaid `tiltedCylinder.ts` intersection logic (11.12.2):
    // - start from `intersect.rect(node, point)`,
    // - then adjust x when the intersection hits the curved ends.
    let mut pos = intersect_rect(node, point);
    let y = pos.y - node.y;
    let top_or_bottom_center =
        (point.x - node.x).abs() < 1e-6 && (pos.x - node.x).abs() < 1e-6 && {
            let half_h = node.height / 2.0;
            (y.abs() - half_h).abs() < 1e-6
        };
    if top_or_bottom_center {
        return pos;
    }

    let ry = node.height / 2.0;
    let rx = if ry == 0.0 {
        0.0
    } else {
        ry / (2.5 + node.height / 50.0)
    };

    if ry != 0.0
        && (y.abs() < node.height / 2.0
            || (y.abs() == node.height / 2.0 && (pos.x - node.x).abs() > node.width / 2.0 - rx))
    {
        let mut x = rx * rx * (1.0 - (y * y) / (ry * ry));
        if x != 0.0 {
            x = x.abs().sqrt();
        }
        x = rx - x;
        if point.x - node.x > 0.0 {
            x = -x;
        }
        pos.x += x;
    }

    pos
}

fn intersect_line(
    p1: crate::model::LayoutPoint,
    p2: crate::model::LayoutPoint,
    q1: crate::model::LayoutPoint,
    q2: crate::model::LayoutPoint,
) -> Option<crate::model::LayoutPoint> {
    // Mermaid 12 `rendering-elements/intersect/intersect-line.js` uses floating-point
    // division without the Graphics Gems integer-rounding bias. That old bias moves
    // an attachment off its ray and creates a spurious terminal bend.
    let a1 = p2.y - p1.y;
    let b1 = p1.x - p2.x;
    let c1 = p2.x * p1.y - p1.x * p2.y;

    let r3 = a1 * q1.x + b1 * q1.y + c1;
    let r4 = a1 * q2.x + b1 * q2.y + c1;

    fn same_sign(r1: f64, r2: f64) -> bool {
        r1 * r2 > 0.0
    }

    if r3 != 0.0 && r4 != 0.0 && same_sign(r3, r4) {
        return None;
    }

    let a2 = q2.y - q1.y;
    let b2 = q1.x - q2.x;
    let c2 = q2.x * q1.y - q1.x * q2.y;

    let r1 = a2 * p1.x + b2 * p1.y + c2;
    let r2 = a2 * p2.x + b2 * p2.y + c2;

    // Preserve the pinned source's asymmetric second-side test, including its
    // acceptance of an intersection beyond the query segment.
    let epsilon = 1e-6;
    if r1.abs() < epsilon && r2.abs() < epsilon && same_sign(r1, r2) {
        return None;
    }

    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }

    Some(crate::model::LayoutPoint {
        x: (b1 * c2 - b2 * c1) / denom,
        y: (a2 * c1 - a1 * c2) / denom,
    })
}

fn intersect_polygon(
    node: &BoundaryNode,
    poly_points: &[crate::model::LayoutPoint],
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    // Port of Mermaid 12 `rendering-elements/intersect/intersect-polygon.js`.
    let x1 = node.x;
    let y1 = node.y;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for p in poly_points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
    }

    let left = x1 - node.width / 2.0 - min_x;
    let top = y1 - node.height / 2.0 - min_y;

    let mut intersections: Vec<crate::model::LayoutPoint> = Vec::new();
    for i in 0..poly_points.len() {
        let p1 = &poly_points[i];
        let p2 = &poly_points[if i + 1 < poly_points.len() { i + 1 } else { 0 }];
        let q1 = crate::model::LayoutPoint {
            x: left + p1.x,
            y: top + p1.y,
        };
        let q2 = crate::model::LayoutPoint {
            x: left + p2.x,
            y: top + p2.y,
        };
        if let Some(inter) = intersect_line(
            crate::model::LayoutPoint { x: x1, y: y1 },
            point.clone(),
            q1,
            q2,
        ) {
            intersections.push(inter);
        }
    }

    if intersections.is_empty() {
        return crate::model::LayoutPoint { x: x1, y: y1 };
    }

    if intersections.len() > 1 {
        intersections.sort_by(|p, q| {
            let pdx = p.x - point.x;
            let pdy = p.y - point.y;
            let qdx = q.x - point.x;
            let qdy = q.y - point.y;
            let dist_p = (pdx * pdx + pdy * pdy).sqrt();
            let dist_q = (qdx * qdx + qdy * qdy).sqrt();
            dist_p
                .partial_cmp(&dist_q)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    intersections[0].clone()
}

fn intersect_polygon_hourglass(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let poly_points = [
        crate::model::LayoutPoint { x: 0.0, y: 0.0 },
        crate::model::LayoutPoint {
            x: node.width.max(1.0),
            y: 0.0,
        },
        crate::model::LayoutPoint {
            x: 0.0,
            y: node.height.max(1.0),
        },
        crate::model::LayoutPoint {
            x: node.width.max(1.0),
            y: node.height.max(1.0),
        },
    ];

    // `hourglass.ts` uses the same polygon intersection as other shapes.
    intersect_polygon(node, &poly_points, point)
}

fn folder_tab_height_from_total_height(total_height: f64) -> f64 {
    // Invert Mermaid's tab rule for the layout dimensions available to edge routing. The
    // measured content height is clamped to an 8px minimum and a 14px maximum.
    if total_height <= 58.0 {
        8.0
    } else if total_height < 101.5 {
        total_height * 0.16 / 1.16
    } else {
        14.0
    }
}

fn folder_polygon_points(node: &BoundaryNode) -> Vec<crate::model::LayoutPoint> {
    let width = node.width.max(1.0);
    let total_height = node.height.max(1.0);
    let tab_height = folder_tab_height_from_total_height(total_height).min(total_height);
    let tab_width = (width * 0.38).max(28.0);
    let top = -total_height / 2.0;
    vec![
        crate::model::LayoutPoint {
            x: -width / 2.0,
            y: top,
        },
        crate::model::LayoutPoint {
            x: -width / 2.0 + tab_width,
            y: top,
        },
        crate::model::LayoutPoint {
            x: -width / 2.0 + tab_width,
            y: top + tab_height,
        },
        crate::model::LayoutPoint {
            x: width / 2.0,
            y: top + tab_height,
        },
        crate::model::LayoutPoint {
            x: width / 2.0,
            y: total_height / 2.0,
        },
        crate::model::LayoutPoint {
            x: -width / 2.0,
            y: total_height / 2.0,
        },
    ]
}

fn bucket_polygon_points_for_dimensions(
    width: f64,
    total_height: f64,
) -> Vec<crate::model::LayoutPoint> {
    let width = width.max(1.0);
    let total_height = total_height.max(1.0);
    let rim_ry = (width * 0.08).clamp(5.0, 12.0);
    let top_y = -total_height / 2.0 + rim_ry;
    let bottom_y = total_height / 2.0;
    let bottom_width = width * 0.72;
    let segments = 12usize;
    let mut points = Vec::with_capacity((segments + 1) * 2);
    for i in 0..=segments {
        let theta = std::f64::consts::PI - (i as f64 * std::f64::consts::PI / segments as f64);
        points.push(crate::model::LayoutPoint {
            x: width / 2.0 * theta.cos(),
            y: top_y - rim_ry * theta.sin(),
        });
    }
    for i in (0..=segments).rev() {
        let theta = std::f64::consts::PI - (i as f64 * std::f64::consts::PI / segments as f64);
        points.push(crate::model::LayoutPoint {
            x: bottom_width / 2.0 * theta.cos(),
            y: bottom_y + rim_ry * theta.sin(),
        });
    }
    points
}

fn bucket_polygon_points(node: &BoundaryNode) -> Vec<crate::model::LayoutPoint> {
    bucket_polygon_points_for_dimensions(node.width, node.height)
}

fn person_polygon_points_for_dimensions(
    width: f64,
    total_height: f64,
) -> Vec<crate::model::LayoutPoint> {
    let width = width.max(1.0);
    let total_height = total_height.max(1.0);
    let head_radius = (width * 0.23).clamp(16.0, 56.0);
    let overlap = head_radius * 0.27;
    let body_height = (total_height - 2.0 * head_radius + overlap).max(0.0);
    let body_radius = (width * 0.177).min(body_height * 0.45);
    let top = -total_height / 2.0;
    let body_top = top + 2.0 * head_radius - overlap;
    let head_center_y = top + head_radius;
    let phi = ((body_top - head_center_y) / head_radius.max(1.0))
        .clamp(-1.0, 1.0)
        .asin();
    let segments = 24usize;
    let mut points = Vec::with_capacity(segments + 4 * 12 + 8);
    for i in 0..=segments {
        let start = (180.0 + phi.to_degrees()).to_radians();
        let end = (-phi.to_degrees()).to_radians();
        let angle = start + i as f64 * (end - start) / segments as f64;
        points.push(crate::model::LayoutPoint {
            x: -(head_radius * angle.cos()),
            y: head_center_y - head_radius * angle.sin(),
        });
    }

    let circle = |cx: f64, cy: f64, start_deg: f64, end_deg: f64| {
        (0..12)
            .map(|i| {
                let angle = (start_deg + (end_deg - start_deg) * i as f64 / 11.0).to_radians();
                crate::model::LayoutPoint {
                    x: -(cx + body_radius * angle.cos()),
                    y: -(cy + body_radius * angle.sin()),
                }
            })
            .collect::<Vec<_>>()
    };
    points.extend(circle(
        -(-width / 2.0 + body_radius),
        -(body_top + body_radius),
        90.0,
        0.0,
    ));
    points.extend(circle(
        -(-width / 2.0 + body_radius),
        -(total_height / 2.0 - body_radius),
        360.0,
        270.0,
    ));
    points.extend(circle(
        -(width / 2.0 - body_radius),
        -(total_height / 2.0 - body_radius),
        270.0,
        180.0,
    ));
    points.extend(circle(
        -(width / 2.0 - body_radius),
        -(body_top + body_radius),
        180.0,
        90.0,
    ));
    points
}

fn person_polygon_points(node: &BoundaryNode) -> Vec<crate::model::LayoutPoint> {
    person_polygon_points_for_dimensions(node.width, node.height)
}

fn polygon_points_for_layout_shape(
    layout_shape: &str,
    node: &BoundaryNode,
) -> Option<Vec<crate::model::LayoutPoint>> {
    let w = node.width.max(1.0);
    let h = node.height.max(1.0);

    match layout_shape {
        "odd" | "rect_left_inv_arrow" => Some(
            crate::flowchart::OddGeometry::from_bounds(node.width, node.height)
                .points
                .into_iter()
                .map(|(x, y)| crate::model::LayoutPoint { x, y })
                .collect(),
        ),
        "subroutine" | "fr-rect" | "subproc" | "subprocess" | "framed-rectangle" => {
            // Port of Mermaid@11.12.2 `subroutine.ts` points used for polygon intersection.
            //
            // Mermaid's insertPolygonShape(...) uses `w = bbox.width + padding` but the
            // resulting bbox expands by `offset*2` (=16px) due to the outer frame.
            let inner_w = (w - 16.0).max(1.0);
            Some(vec![
                crate::model::LayoutPoint { x: 0.0, y: 0.0 },
                crate::model::LayoutPoint { x: inner_w, y: 0.0 },
                crate::model::LayoutPoint { x: inner_w, y: -h },
                crate::model::LayoutPoint { x: 0.0, y: -h },
                crate::model::LayoutPoint { x: 0.0, y: 0.0 },
                crate::model::LayoutPoint { x: -8.0, y: 0.0 },
                crate::model::LayoutPoint {
                    x: inner_w + 8.0,
                    y: 0.0,
                },
                crate::model::LayoutPoint {
                    x: inner_w + 8.0,
                    y: -h,
                },
                crate::model::LayoutPoint { x: -8.0, y: -h },
                crate::model::LayoutPoint { x: -8.0, y: 0.0 },
            ])
        }
        "lean_right" | "lean-r" | "lean-right" | "in-out" => Some(
            crate::flowchart::LeanGeometry::from_bounds(
                crate::flowchart::LeanKind::Right,
                node.width,
                node.height,
            )
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect(),
        ),
        "lean_left" | "lean-l" | "lean-left" | "out-in" => Some(
            crate::flowchart::LeanGeometry::from_bounds(
                crate::flowchart::LeanKind::Left,
                node.width,
                node.height,
            )
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect(),
        ),
        "trapezoid" | "trap-b" | "priority" | "trapezoid-bottom" => Some(
            crate::flowchart::LeanGeometry::from_bounds(
                crate::flowchart::LeanKind::Trapezoid,
                node.width,
                node.height,
            )
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect(),
        ),
        "inv_trapezoid" | "inv-trapezoid" | "trap-t" | "manual" | "trapezoid-top" => Some(
            crate::flowchart::LeanGeometry::from_bounds(
                crate::flowchart::LeanKind::InvertedTrapezoid,
                node.width,
                node.height,
            )
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect(),
        ),
        "tri" | "extract" | "triangle" => Some(vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: h, y: 0.0 },
            crate::model::LayoutPoint { x: h / 2.0, y: -h },
        ]),
        "manual-file" | "flipped-triangle" | "flip-tri" => Some(vec![
            crate::model::LayoutPoint { x: 0.0, y: -h },
            crate::model::LayoutPoint { x: h, y: -h },
            crate::model::LayoutPoint { x: h / 2.0, y: 0.0 },
        ]),
        "manual-input" | "sloped-rectangle" | "sl-rect" => {
            let base_h = (h / 1.5).max(1.0);
            let x = -w / 2.0;
            let y = -base_h / 2.0;
            Some(vec![
                crate::model::LayoutPoint { x, y },
                crate::model::LayoutPoint { x, y: y + base_h },
                crate::model::LayoutPoint {
                    x: x + w,
                    y: y + base_h,
                },
                crate::model::LayoutPoint {
                    x: x + w,
                    y: y - base_h / 2.0,
                },
            ])
        }
        "hourglass" | "collate" => Some(vec![
            crate::model::LayoutPoint { x: 0.0, y: 0.0 },
            crate::model::LayoutPoint { x: w, y: 0.0 },
            crate::model::LayoutPoint { x: 0.0, y: h },
            crate::model::LayoutPoint { x: w, y: h },
        ]),
        "bucket" => Some(bucket_polygon_points(node)),
        "folder" | "directory" => Some(folder_polygon_points(node)),
        "person" => Some(person_polygon_points(node)),
        _ => None,
    }
}

fn intersect_wave_rectangle(
    node: &BoundaryNode,
    width: f64,
    height: f64,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    // `waveRectangle.ts` intersects the two sampled sine waves, rather than their bbox.
    let amplitude = height / 8.0;
    let final_height = height + amplitude * 2.0;
    let mut points = Vec::with_capacity(104);
    for (left, right, y, cycles) in [
        (-width / 2.0, width / 2.0, final_height / 2.0, 1.0),
        (width / 2.0, -width / 2.0, -final_height / 2.0, -1.0),
    ] {
        points.push(crate::model::LayoutPoint { x: left, y });
        points.extend(
            super::super::render::node::geom::generate_full_sine_wave_points(
                left, y, right, y, amplitude, cycles,
            )
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y }),
        );
    }
    intersect_polygon(node, &points, point)
}

fn compute_node_label_metrics_for_intersection(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
) -> Option<crate::text::TextMetrics> {
    let flow_node = ctx.nodes_by_id.get(node_id)?;
    let label_text = ctx
        .model
        .node_label_for_render(flow_node)
        .unwrap_or_default();
    let label_type = flow_node.label_type.as_deref().unwrap_or("text");

    let label_base_style = if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike {
        &ctx.html_label_text_style
    } else {
        &ctx.text_style
    };
    let node_text_style = crate::flowchart::flowchart_effective_text_style_for_node_classes(
        label_base_style,
        ctx.class_defs,
        &flow_node.classes,
        &flow_node.styles,
    );
    let min_width = if ctx.subgraphs_by_id.contains_key(node_id) {
        0.0
    } else {
        crate::flowchart::flowchart_node_label_min_width(
            label_text,
            flow_node.layout_shape.as_deref(),
            ctx.config,
        )
    };
    if let Some(metrics) = ctx.svg_label_sidecar.and_then(|sidecar| {
        let owner = sidecar.node_owner(node_id, ctx.swimlane_direction.is_some())?;
        sidecar.prepared_metrics(
            owner,
            label_text,
            ctx.measurer,
            node_text_style.as_ref(),
            Some(ctx.wrapping_width),
            true,
            crate::flowchart::FlowchartSvgWidthMode::Bbox,
        )
    }) {
        return Some(metrics.with_label_min_width(label_text, min_width, None));
    }
    let metrics = crate::flowchart::flowchart_label_metrics_for_layout(
        crate::flowchart::FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label: label_text,
            label_type,
            style: &node_text_style,
            max_width_px: Some(ctx.wrapping_width),
            wrap_mode: ctx.node_wrap_mode,
            config: ctx.config,
            math_renderer: ctx.math_renderer,
        },
    );

    Some(metrics.with_label_min_width(label_text, min_width, None))
}

fn intersect_curly_brace(
    node: &BoundaryNode,
    shape: &str,
    label_width: f64,
    label_height: f64,
    padding: f64,
    look_is_neo: bool,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let polygon = super::super::render::node::shapes::curly_brace_comment_intersection_points(
        shape,
        label_width,
        label_height,
        padding,
        look_is_neo,
    );
    // Mermaid 12 assigns intersect.polygon(node, rectPoints, point) before the
    // visible brace group's translation. Reuse those unshifted polygon points,
    // including the left-only brace's width-dependent central indentation.
    let points: Vec<_> = polygon
        .into_iter()
        .map(|(x, y)| crate::model::LayoutPoint { x, y })
        .collect();
    intersect_polygon(node, &points, point)
}

fn intersect_notched_rectangle(
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let width = node.width.max(1.0);
    let height = node.height.max(1.0);
    let notch = 12.0;
    let half_width = width / 2.0;
    let half_height = height / 2.0;
    let points = vec![
        crate::model::LayoutPoint {
            x: -half_width + notch,
            y: -half_height,
        },
        crate::model::LayoutPoint {
            x: half_width,
            y: -half_height,
        },
        crate::model::LayoutPoint {
            x: half_width,
            y: half_height,
        },
        crate::model::LayoutPoint {
            x: -half_width,
            y: half_height,
        },
        crate::model::LayoutPoint {
            x: -half_width,
            y: -half_height + notch,
        },
    ];
    intersect_polygon(node, &points, point)
}

fn stacked_rectangle_intersection_points(
    label_width: f64,
    label_height: f64,
    padding: f64,
    look_is_neo: bool,
) -> Vec<crate::model::LayoutPoint> {
    let padding_x = if look_is_neo { 16.0 } else { padding };
    let padding_y = if look_is_neo { 12.0 } else { padding };
    let rect_offset = if look_is_neo { 10.0 } else { 5.0 };
    let width = (label_width + 2.0 * padding_x).max(0.0);
    let height = (label_height + 2.0 * padding_y).max(0.0);
    let x = -width / 2.0;
    let y = -height / 2.0;

    [
        (x - rect_offset, y + rect_offset),
        (x - rect_offset, y + height + rect_offset),
        (x + width - rect_offset, y + height + rect_offset),
        (x + width - rect_offset, y + height),
        (x + width, y + height),
        (x + width, y + height - rect_offset),
        (x + width + rect_offset, y + height - rect_offset),
        (x + width + rect_offset, y - rect_offset),
        (x + rect_offset, y - rect_offset),
        (x + rect_offset, y),
        (x, y),
        (x, y + rect_offset),
    ]
    .into_iter()
    .map(|(x, y)| crate::model::LayoutPoint { x, y })
    .collect()
}

fn intersect_stacked_rectangle(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    node: &BoundaryNode,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
        return intersect_rect(node, point);
    };
    let points = stacked_rectangle_intersection_points(
        metrics.width,
        metrics.height,
        ctx.node_padding,
        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
    );
    intersect_polygon(node, &points, point)
}

pub(in crate::svg::parity::flowchart) fn intersect_for_layout_shape(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    node: &BoundaryNode,
    layout_shape: Option<&str>,
    point: &crate::model::LayoutPoint,
) -> crate::model::LayoutPoint {
    fn generate_full_sine_wave_points(
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        amplitude: f64,
        num_cycles: f64,
    ) -> Vec<crate::model::LayoutPoint> {
        let steps: usize = 50;
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;
        let cycle_length = delta_x / num_cycles;
        let frequency = (2.0 * std::f64::consts::PI) / cycle_length;
        let mid_y = y1 + delta_y / 2.0;

        let mut points: Vec<crate::model::LayoutPoint> = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = (i as f64) / (steps as f64);
            let x = x1 + t * delta_x;
            let y = mid_y + amplitude * (frequency * (x - x1)).sin();
            points.push(crate::model::LayoutPoint { x, y });
        }
        points
    }

    fn arc_points(
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        rx: f64,
        ry: f64,
        clockwise: bool,
    ) -> Vec<crate::model::LayoutPoint> {
        let num_points: usize = 20;

        let mid_x = (x1 + x2) / 2.0;
        let mid_y = (y1 + y2) / 2.0;
        let angle = (y2 - y1).atan2(x2 - x1);

        let dx = (x2 - x1) / 2.0;
        let dy = (y2 - y1) / 2.0;
        let transformed_x = dx / rx;
        let transformed_y = dy / ry;
        let distance = (transformed_x * transformed_x + transformed_y * transformed_y).sqrt();
        if distance > 1.0 {
            return vec![
                crate::model::LayoutPoint { x: x1, y: y1 },
                crate::model::LayoutPoint { x: x2, y: y2 },
            ];
        }

        let scaled_center_distance = (1.0 - distance * distance).sqrt();
        let sign = if clockwise { -1.0 } else { 1.0 };
        let center_x = mid_x + scaled_center_distance * ry * angle.sin() * sign;
        let center_y = mid_y - scaled_center_distance * rx * angle.cos() * sign;

        let start_angle = ((y1 - center_y) / ry).atan2((x1 - center_x) / rx);
        let end_angle = ((y2 - center_y) / ry).atan2((x2 - center_x) / rx);

        let mut angle_range = end_angle - start_angle;
        if clockwise && angle_range < 0.0 {
            angle_range += 2.0 * std::f64::consts::PI;
        }
        if !clockwise && angle_range > 0.0 {
            angle_range -= 2.0 * std::f64::consts::PI;
        }

        let mut points: Vec<crate::model::LayoutPoint> = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let t = i as f64 / (num_points - 1) as f64;
            let a = start_angle + t * angle_range;
            let x = center_x + rx * a.cos();
            let y = center_y + ry * a.sin();
            points.push(crate::model::LayoutPoint { x, y });
        }
        points
    }

    fn intersect_stadium(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        // Port of Mermaid@11.12.2 `stadium.ts` intersection behavior:
        // - `points` are generated from the theoretical render dimensions,
        // - `node.width/height` used by `intersect.polygon(...)` come from `updateNodeBounds(...)`.
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };
        let geometry = crate::flowchart::StadiumGeometry::from_label(
            metrics.width,
            metrics.height,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let points: Vec<_> = geometry
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect();
        intersect_polygon(node, &points, point)
    }

    fn intersect_folder(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_polygon(node, &folder_polygon_points(node), point);
        };
        let (render_w, render_h) = crate::flowchart::flowchart_node_render_dimensions(
            Some("folder"),
            metrics,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let width = render_w.max(node.width).max(1.0);
        let total_height = render_h.max(node.height).max(1.0);
        let content_height = metrics.height + 2.0 * ctx.node_padding.max(0.0);
        let tab_height = (content_height * 0.16).clamp(8.0, 14.0).min(total_height);
        let tab_width = (width * 0.38).max(28.0);
        let top = -total_height / 2.0;
        let points = vec![
            crate::model::LayoutPoint {
                x: -width / 2.0,
                y: top,
            },
            crate::model::LayoutPoint {
                x: -width / 2.0 + tab_width,
                y: top,
            },
            crate::model::LayoutPoint {
                x: -width / 2.0 + tab_width,
                y: top + tab_height,
            },
            crate::model::LayoutPoint {
                x: width / 2.0,
                y: top + tab_height,
            },
            crate::model::LayoutPoint {
                x: width / 2.0,
                y: total_height / 2.0,
            },
            crate::model::LayoutPoint {
                x: -width / 2.0,
                y: total_height / 2.0,
            },
        ];
        intersect_polygon(node, &points, point)
    }

    fn intersect_bucket(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_polygon(node, &bucket_polygon_points(node), point);
        };
        let (render_w, render_h) = crate::flowchart::flowchart_node_render_dimensions(
            Some("bucket"),
            metrics,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let width = render_w.max(node.width).max(1.0);
        let total_height = render_h.max(node.height).max(1.0);
        let points = bucket_polygon_points_for_dimensions(width, total_height);
        intersect_polygon(node, &points, point)
    }

    fn intersect_person(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_polygon(node, &person_polygon_points(node), point);
        };
        let (render_w, render_h) = crate::flowchart::flowchart_node_render_dimensions(
            Some("person"),
            metrics,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let width = render_w.max(node.width).max(1.0);
        let total_height = render_h.max(node.height).max(1.0);
        let points = person_polygon_points_for_dimensions(width, total_height);
        intersect_polygon(node, &points, point)
    }

    fn intersect_hexagon(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        // Source points precede updateNodeBounds; prefer the measured label when available.
        let geometry = match compute_node_label_metrics_for_intersection(ctx, node_id) {
            Some(metrics) => crate::flowchart::HexagonGeometry::from_label(
                metrics.width,
                metrics.height,
                ctx.node_padding,
                neo,
            ),
            None => crate::flowchart::HexagonGeometry::from_bounds(node.width, node.height, neo),
        };
        let points: Vec<_> = geometry
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect();
        intersect_polygon(node, &points, point)
    }

    fn intersect_curved_trapezoid(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };
        let geometry = crate::flowchart::DisplayGeometry::from_label(
            metrics.width,
            metrics.height,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let points: Vec<_> = geometry
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect();
        intersect_polygon(node, &points, point)
    }

    fn intersect_bow_tie_rect(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if neo { 16.0 } else { p };
        let padding_y = if neo { 12.0 } else { p };
        let w = metrics.width + 2.0 * padding_x;
        let h = metrics.height + padding_y;
        let ry = h / 2.0;
        let rx = ry / (2.5 + h / 50.0);

        let mut points: Vec<crate::model::LayoutPoint> = Vec::new();
        points.push(crate::model::LayoutPoint {
            x: w / 2.0,
            y: -h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0,
            y: -h / 2.0,
        });
        points.extend(arc_points(
            -w / 2.0,
            -h / 2.0,
            -w / 2.0,
            h / 2.0,
            rx,
            ry,
            false,
        ));
        points.push(crate::model::LayoutPoint {
            x: w / 2.0,
            y: h / 2.0,
        });
        points.extend(arc_points(
            w / 2.0,
            h / 2.0,
            w / 2.0,
            -h / 2.0,
            rx,
            ry,
            true,
        ));

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in &points {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }

        let render_node = BoundaryNode {
            x: node.x,
            y: node.y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        };

        intersect_polygon(&render_node, &points, point)
    }

    fn intersect_tagged_document(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let w = metrics.width + 2.0 * p;
        let h = metrics.height + 2.0 * p;
        let wave_amplitude = h / 8.0;
        let final_h = h + wave_amplitude;
        let ext = (w / 2.0) * 0.1;

        let mut points: Vec<crate::model::LayoutPoint> = Vec::new();
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - ext,
            y: final_h / 2.0,
        });
        points.extend(generate_full_sine_wave_points(
            -w / 2.0 - ext,
            final_h / 2.0,
            w / 2.0 + ext,
            final_h / 2.0,
            wave_amplitude,
            0.8,
        ));
        points.push(crate::model::LayoutPoint {
            x: w / 2.0 + ext,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - ext,
            y: -final_h / 2.0,
        });

        intersect_polygon(node, &points, point)
    }

    fn intersect_tagged_rect(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let look_is_neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if look_is_neo { 16.0 } else { p };
        let padding_y = if look_is_neo { 12.0 } else { p };
        let w = metrics.width + 2.0 * padding_x;
        let h = metrics.height + 2.0 * padding_y;
        let x = -w / 2.0;
        let y = -h / 2.0;
        let tag_width = 0.2 * h;

        let points = vec![
            crate::model::LayoutPoint {
                x: x - tag_width / 2.0,
                y,
            },
            crate::model::LayoutPoint {
                x: x + w + tag_width / 2.0,
                y,
            },
            crate::model::LayoutPoint {
                x: x + w + tag_width / 2.0,
                y: y + h,
            },
            crate::model::LayoutPoint {
                x: x - tag_width / 2.0,
                y: y + h,
            },
        ];

        intersect_polygon(node, &points, point)
    }

    fn intersect_wave_document(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let look_is_neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if look_is_neo { 16.0 } else { p };
        let padding_y = if look_is_neo { 12.0 } else { p };
        let w = (metrics.width + 2.0 * padding_x).max(0.0);
        let h = (metrics.height + 2.0 * padding_y).max(0.0);
        let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
        let final_h = h + wave_amplitude;
        let extra_w = ((14.0 - w).max(0.0)) / 2.0;

        let mut points: Vec<crate::model::LayoutPoint> = Vec::new();
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - extra_w,
            y: final_h / 2.0,
        });
        points.extend(generate_full_sine_wave_points(
            -w / 2.0 - extra_w,
            final_h / 2.0,
            w / 2.0 + extra_w,
            final_h / 2.0,
            wave_amplitude,
            0.8,
        ));
        points.push(crate::model::LayoutPoint {
            x: w / 2.0 + extra_w,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - extra_w,
            y: -final_h / 2.0,
        });

        intersect_polygon(node, &points, point)
    }

    fn intersect_lined_wave_document(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let look_is_neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if look_is_neo { 16.0 } else { ctx.node_padding };
        let padding_y = if look_is_neo { 12.0 } else { ctx.node_padding };
        let w = (metrics.width + 2.0 * padding_x).max(0.0);
        let h = (metrics.height + 2.0 * padding_y).max(0.0);
        let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
        let final_h = h + wave_amplitude;
        let extension = (w / 2.0) * 0.1;

        let mut points: Vec<crate::model::LayoutPoint> = Vec::new();
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - extension,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - extension,
            y: final_h / 2.0,
        });
        points.extend(generate_full_sine_wave_points(
            -w / 2.0 - extension,
            final_h / 2.0,
            w / 2.0 + extension,
            final_h / 2.0,
            wave_amplitude,
            0.8,
        ));
        points.push(crate::model::LayoutPoint {
            x: w / 2.0 + extension,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0 - extension,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0,
            y: -final_h / 2.0,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0,
            y: (final_h / 2.0) * 1.1,
        });
        points.push(crate::model::LayoutPoint {
            x: -w / 2.0,
            y: -final_h / 2.0,
        });

        intersect_polygon(node, &points, point)
    }

    fn intersect_delay(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };
        let geometry = crate::flowchart::DelayGeometry::from_label(
            metrics.width,
            metrics.height,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let points: Vec<_> = geometry
            .points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect();
        intersect_polygon(node, &points, point)
    }

    fn intersect_notched_pentagon(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if neo { 16.0 } else { p };
        let padding_y = if neo { 12.0 } else { p };
        let w = metrics.width + 2.0 * padding_x;
        let h = metrics.height + 2.0 * padding_y;
        let points = vec![
            crate::model::LayoutPoint {
                x: (-w / 2.0) * 0.8,
                y: -h / 2.0,
            },
            crate::model::LayoutPoint {
                x: (w / 2.0) * 0.8,
                y: -h / 2.0,
            },
            crate::model::LayoutPoint {
                x: w / 2.0,
                y: (-h / 2.0) * 0.6,
            },
            crate::model::LayoutPoint {
                x: w / 2.0,
                y: h / 2.0,
            },
            crate::model::LayoutPoint {
                x: -w / 2.0,
                y: h / 2.0,
            },
            crate::model::LayoutPoint {
                x: -w / 2.0,
                y: (-h / 2.0) * 0.6,
            },
        ];

        intersect_polygon(node, &points, point)
    }

    fn intersect_multi_wave_edged_rect(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let geometry = crate::flowchart::flowchart_stacked_document_geometry(
            metrics.width,
            metrics.height,
            ctx.node_padding,
            crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
        );
        let points: Vec<_> = geometry
            .outer_points
            .into_iter()
            .map(|(x, y)| crate::model::LayoutPoint { x, y })
            .collect();

        intersect_polygon(node, &points, point)
    }

    fn intersect_lightning_bolt(
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let width = 35.0;
        let height = 35.0;
        let gap = 7.0;
        let points = vec![
            crate::model::LayoutPoint { x: width, y: 0.0 },
            crate::model::LayoutPoint {
                x: 0.0,
                y: height + gap / 2.0,
            },
            crate::model::LayoutPoint {
                x: width - 2.0 * gap,
                y: height + gap / 2.0,
            },
            crate::model::LayoutPoint {
                x: 0.0,
                y: 2.0 * height,
            },
            crate::model::LayoutPoint {
                x: width,
                y: height - gap / 2.0,
            },
            crate::model::LayoutPoint {
                x: 2.0 * gap,
                y: height - gap / 2.0,
            },
        ];

        intersect_polygon(node, &points, point)
    }

    fn intersect_window_pane(
        ctx: &FlowchartRenderCtx<'_>,
        node_id: &str,
        node: &BoundaryNode,
        point: &crate::model::LayoutPoint,
    ) -> crate::model::LayoutPoint {
        let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
            return intersect_rect(node, point);
        };

        let p = ctx.node_padding;
        let look_is_neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
        let padding_x = if look_is_neo { 16.0 } else { p };
        let padding_y = if look_is_neo { 12.0 } else { p };
        let w = metrics.width + 2.0 * padding_x;
        let h = metrics.height + 2.0 * padding_y;
        let rect_offset = 10.0;
        let x = -w / 2.0;
        let y = -h / 2.0;
        let points = vec![
            crate::model::LayoutPoint {
                x: x - rect_offset,
                y: y - rect_offset,
            },
            crate::model::LayoutPoint {
                x: x - rect_offset,
                y: y + h,
            },
            crate::model::LayoutPoint { x: x + w, y: y + h },
            crate::model::LayoutPoint {
                x: x + w,
                y: y - rect_offset,
            },
        ];

        intersect_polygon(node, &points, point)
    }

    match layout_shape {
        Some(shape @ ("brace" | "brace-l" | "comment" | "brace-r" | "braces")) => {
            let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
                return intersect_rect(node, point);
            };
            intersect_curly_brace(
                node,
                shape,
                metrics.width,
                metrics.height,
                ctx.node_padding,
                crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
                point,
            )
        }
        Some(shape @ ("circle" | "circ" | "doublecircle" | "dbl-circ" | "double-circle")) => {
            let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
                return intersect_circle(node, node.width / 2.0, point);
            };
            let (diameter, _) = crate::flowchart::flowchart_node_render_dimensions(
                Some(shape),
                metrics,
                ctx.node_padding,
                crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
            );
            intersect_circle(node, diameter / 2.0, point)
        }
        // These RoughJS shapes capture a fixed radius before measuring their painted bounds.
        Some("anchor") => intersect_circle(node, 1.0, point),
        Some("f-circ" | "junction" | "filled-circle") => intersect_circle(node, 7.0, point),
        Some("cross-circ" | "summary" | "crossed-circle") => {
            intersect_circle(node, crate::flowchart::CROSSED_CIRCLE_RADIUS, point)
        }
        Some("sm-circ" | "small-circle" | "start" | "fr-circ" | "framed-circle" | "stop") => {
            intersect_circle(node, node.width / 2.0, point)
        }
        Some("choice") => {
            let half = node.width.max(28.0) / 2.0;
            let points = [(0.0, half), (half, 0.0), (0.0, -half), (-half, 0.0)]
                .map(|(x, y)| crate::model::LayoutPoint { x, y });
            intersect_polygon(node, &points, point)
        }
        Some("cylinder" | "cyl" | "db" | "database") => intersect_cylinder(node, point),
        Some("lin-cyl" | "disk" | "lined-cylinder") => intersect_cylinder(node, point),
        Some("h-cyl" | "das" | "horizontal-cylinder") => intersect_tilted_cylinder(node, point),
        Some("hourglass" | "collate") => intersect_polygon_hourglass(node, point),
        Some("diamond" | "diam" | "question" | "decision") => intersect_diamond(node, point),
        Some("stadium" | "terminal" | "pill") => intersect_stadium(ctx, node_id, node, point),
        Some("hexagon" | "hex" | "prepare") => intersect_hexagon(ctx, node_id, node, point),
        Some("curv-trap" | "display" | "curved-trapezoid") => {
            intersect_curved_trapezoid(ctx, node_id, node, point)
        }
        Some("bow-rect" | "stored-data" | "bow-tie-rectangle") => {
            intersect_bow_tie_rect(ctx, node_id, node, point)
        }
        Some("tag-rect" | "tagged-rectangle" | "tag-proc" | "tagged-process") => {
            intersect_tagged_rect(ctx, node_id, node, point)
        }
        Some("paper-tape" | "flag") => {
            let Some(metrics) = compute_node_label_metrics_for_intersection(ctx, node_id) else {
                return intersect_rect(node, point);
            };
            let look_is_neo = crate::config::mermaid_config_diagram_look(ctx.config).is_neo();
            let padding_x = if look_is_neo { 16.0 } else { ctx.node_padding };
            let padding_y = if look_is_neo { 20.0 } else { ctx.node_padding };
            intersect_wave_rectangle(
                node,
                (metrics.width + 2.0 * padding_x).max(0.0),
                (metrics.height + padding_y).max(0.0),
                point,
            )
        }
        Some("notch-rect" | "notched-rectangle" | "card") => {
            intersect_notched_rectangle(node, point)
        }
        Some("st-rect" | "procs" | "processes" | "stacked-rectangle") => {
            intersect_stacked_rectangle(ctx, node_id, node, point)
        }
        Some("doc" | "document") => intersect_wave_document(ctx, node_id, node, point),
        Some("lin-doc" | "lined-document") => {
            intersect_lined_wave_document(ctx, node_id, node, point)
        }
        Some("delay" | "half-rounded-rectangle") => intersect_delay(ctx, node_id, node, point),
        Some("notch-pent" | "loop-limit" | "notched-pentagon") => {
            intersect_notched_pentagon(ctx, node_id, node, point)
        }
        Some("docs" | "documents" | "st-doc" | "stacked-document") => {
            intersect_multi_wave_edged_rect(ctx, node_id, node, point)
        }
        Some("tag-doc" | "tagged-document") => intersect_tagged_document(ctx, node_id, node, point),
        Some("bolt" | "com-link" | "lightning-bolt") => intersect_lightning_bolt(node, point),
        Some("win-pane" | "internal-storage" | "window-pane") => {
            intersect_window_pane(ctx, node_id, node, point)
        }
        Some("folder" | "directory") => intersect_folder(ctx, node_id, node, point),
        Some("bucket") => intersect_bucket(ctx, node_id, node, point),
        Some("person") => intersect_person(ctx, node_id, node, point),
        Some(s) if is_polygon_layout_shape(Some(s)) => polygon_points_for_layout_shape(s, node)
            .map(|pts| intersect_polygon(node, &pts, point))
            .unwrap_or_else(|| intersect_rect(node, point)),
        _ => intersect_rect(node, point),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BoundaryNode, force_intersect_for_layout_shape, intersect_circle, intersect_curly_brace,
        intersect_line, intersect_notched_rectangle, intersect_polygon,
        intersect_polygon_hourglass, intersect_wave_rectangle,
        stacked_rectangle_intersection_points,
    };
    use crate::model::LayoutPoint;

    fn point(x: f64, y: f64) -> LayoutPoint {
        LayoutPoint { x, y }
    }

    #[test]
    fn circle_intersection_preserves_the_source_radius_despite_measured_bounds() {
        let node = BoundaryNode {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 20.0,
        };
        let actual = intersect_circle(&node, 20.0, &point(100.0, 100.0));
        assert!((actual.x - 14.142135623730951).abs() < 1e-12, "{actual:?}");
        assert!((actual.y - 14.142135623730951).abs() < 1e-12, "{actual:?}");
        let center = intersect_circle(&node, 20.0, &point(0.0, 0.0));
        assert!(center.x.is_nan() && center.y.is_nan());
    }

    #[test]
    fn paper_tape_intersects_the_sampled_wave_instead_of_the_bounding_box() {
        // Pinned source: label 100x20, Neo padding 16/20 produces base 132x40;
        // the two sine waves have amplitude 5 and baselines at +/-25.
        let node = BoundaryNode {
            x: 0.0,
            y: 0.0,
            width: 132.0,
            height: 59.98026728428272,
        };
        for shape in ["paper-tape", "flag"] {
            assert!(force_intersect_for_layout_shape(Some(shape)));
        }
        for (target, expected) in [
            (point(0.0, -500.0), point(0.0, -25.0)),
            (point(0.0, 500.0), point(0.0, 25.0)),
            (point(-500.0, 0.0), point(-66.0, 0.0)),
            (point(500.0, 0.0), point(66.0, 0.0)),
        ] {
            let actual = intersect_wave_rectangle(&node, 132.0, 40.0, &target);
            assert!((actual.x - expected.x).abs() < 1e-9, "{actual:?}");
            assert!((actual.y - expected.y).abs() < 1e-9, "{actual:?}");
        }
    }

    #[test]
    fn lined_documents_force_source_polygon_intersections() {
        assert!(force_intersect_for_layout_shape(Some("lin-doc")));
        assert!(force_intersect_for_layout_shape(Some("lined-document")));
    }

    #[test]
    fn stacked_rectangles_intersect_the_source_layered_polygon() {
        for shape in ["st-rect", "procs", "processes", "stacked-rectangle"] {
            assert!(force_intersect_for_layout_shape(Some(shape)));
        }
        for (neo, width, height, expected_x, expected_y) in [
            (false, 140.0, 60.0, -60.0, -25.714285714285715),
            (true, 152.0, 64.0, -56.0, -23.57894736842105),
        ] {
            let node = BoundaryNode {
                x: 0.0,
                y: 0.0,
                width,
                height,
            };
            let points = stacked_rectangle_intersection_points(100.0, 20.0, 15.0, neo);
            let actual = intersect_polygon(&node, &points, &point(-width * 5.0, -height * 5.0));
            // The upper-left ray hits a layer step before the bounding-box corner.
            assert!((actual.x - expected_x).abs() < 1e-9, "{actual:?}");
            assert!((actual.y - expected_y).abs() < 1e-9, "{actual:?}");
        }
    }

    #[test]
    fn cards_intersect_the_source_notched_polygon() {
        for shape in ["notch-rect", "notched-rectangle", "card"] {
            assert!(force_intersect_for_layout_shape(Some(shape)));
            let node = BoundaryNode {
                x: 0.0,
                y: 0.0,
                width: 156.0,
                height: 68.0,
            };
            let actual = intersect_notched_rectangle(&node, &point(-780.0, -340.0));
            // The diagonal clips the twelve-pixel notch, before the bounding-box corner.
            assert!((actual.x - (-69.64285714285714)).abs() < 1e-9, "{actual:?}");
            assert!(
                (actual.y - (-30.357142857142858)).abs() < 1e-9,
                "{actual:?}"
            );
        }
    }

    #[test]
    fn mermaid12_braces_intersect_the_source_polygon_in_each_direction() {
        // label + padding gives w=160, h=40 and r=5. The left brace's
        // rectPoints use w*0.1, unlike the visible path's 2*r indentation.
        // Its polygon therefore extends beyond the measured visible bounds.
        for (shape, width, left, right) in [
            ("brace", 170.0, -85.0, 91.0),
            ("brace-l", 170.0, -85.0, 91.0),
            ("comment", 170.0, -85.0, 91.0),
            ("brace-r", 170.0, -85.0, 85.0),
            ("braces", 172.5, -86.25, 86.25),
        ] {
            assert!(force_intersect_for_layout_shape(Some(shape)));
            for (center_x, center_y) in [(0.0, 0.0), (123.0, -47.0)] {
                let node = BoundaryNode {
                    x: center_x,
                    y: center_y,
                    width,
                    height: 50.0,
                };
                for (dx, dy, expected_x, expected_y) in [
                    (-500.0, 0.0, left, 0.0),
                    (500.0, 0.0, right, 0.0),
                    (0.0, -500.0, 0.0, -25.0),
                    (0.0, 500.0, 0.0, 25.0),
                ] {
                    let actual = intersect_curly_brace(
                        &node,
                        shape,
                        145.0,
                        25.0,
                        15.0,
                        false,
                        &point(center_x + dx, center_y + dy),
                    );
                    assert!(
                        (actual.x - center_x - expected_x).abs() < 1e-9
                            && (actual.y - center_y - expected_y).abs() < 1e-9,
                        "{shape}: direction ({dx}, {dy}), actual {actual:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn mermaid12_brace_intersections_preserve_empty_and_neo_dimensions() {
        for (shape, zero_width, neo_width, neo_height) in [
            ("brace", 15.0, 129.8, 42.0),
            ("brace-l", 15.0, 129.8, 42.0),
            ("comment", 15.0, 129.8, 42.0),
            ("brace-r", 10.0, 146.0, 54.0),
            ("braces", 12.5, 148.5, 54.0),
        ] {
            for padding in [0.0, -2.0] {
                let node = BoundaryNode {
                    x: 0.0,
                    y: 0.0,
                    width: zero_width,
                    height: 10.0,
                };
                let actual = intersect_curly_brace(
                    &node,
                    shape,
                    0.0,
                    0.0,
                    padding,
                    false,
                    &point(0.0, 500.0),
                );
                // The empty left brace's visible path widens the measured
                // box beyond rectPoints; pinned polygon normalization keeps
                // its lower vertical intersection at 2.5, not the box's 5.
                let expected_y = if matches!(shape, "brace" | "brace-l" | "comment") {
                    2.5
                } else {
                    5.0
                };
                assert!(
                    actual.x.abs() < 1e-9 && (actual.y - expected_y).abs() < 1e-9,
                    "{shape}: {actual:?}"
                );
            }
            let node = BoundaryNode {
                x: 0.0,
                y: 0.0,
                width: neo_width,
                height: neo_height,
            };
            for direction in [-1.0, 1.0] {
                let actual = intersect_curly_brace(
                    &node,
                    shape,
                    100.0,
                    20.0,
                    15.0,
                    true,
                    &point(0.0, direction * 500.0),
                );
                assert!(
                    actual.x.abs() < 1e-9 && (actual.y - direction * neo_height / 2.0).abs() < 1e-9,
                    "{shape}: {actual:?}"
                );
            }
        }
    }

    #[test]
    fn stacked_document_intersection_uses_pinned_outer_polygon() {
        for (neo, left, right, top, bottom) in [
            (false, -65.0, 85.0, -28.590863482602394, 62.414677511183605),
            (true, -66.0, 86.0, -28.495026308484125, 57.93185341111228),
        ] {
            let geometry =
                crate::flowchart::flowchart_stacked_document_geometry(100.0, 20.0, 15.0, neo);
            let (width, height) = crate::flowchart::flowchart_node_render_dimensions(
                Some("documents"),
                crate::text::TextMetrics {
                    width: 100.0,
                    height: 20.0,
                    line_count: 1,
                },
                15.0,
                neo,
            );
            let node = BoundaryNode {
                x: 10.0,
                y: 20.0,
                width,
                height,
            };
            let polygon: Vec<_> = geometry
                .outer_points
                .into_iter()
                .map(|(x, y)| point(x, y))
                .collect();
            for (query, expected) in [
                (point(-500.0, 20.0), point(left, 20.0)),
                (point(500.0, 20.0), point(right, 20.0)),
                (point(10.0, -500.0), point(10.0, top)),
                (point(10.0, 500.0), point(10.0, bottom)),
            ] {
                let actual = intersect_polygon(&node, &polygon, &query);
                assert!(
                    (actual.x - expected.x).abs() < 1e-9 && (actual.y - expected.y).abs() < 1e-9,
                    "neo={neo}: {actual:?}"
                );
            }
        }
    }

    #[test]
    fn mermaid12_line_intersection_preserves_axis_in_every_quadrant() {
        for (x, y) in [(4.0, 6.0), (-4.0, 6.0), (4.0, -6.0), (-4.0, -6.0)] {
            for reverse in [false, true] {
                let (left, right) = if reverse {
                    (x + 2.0, x - 2.0)
                } else {
                    (x - 2.0, x + 2.0)
                };
                let intersection = intersect_line(
                    point(x, y - 3.0),
                    point(x, y + 3.0),
                    point(left, y),
                    point(right, y),
                )
                .expect("perpendicular segments intersect");
                assert_eq!((intersection.x, intersection.y), (x, y));
            }
        }
    }

    #[test]
    fn mermaid12_line_intersection_keeps_pinned_second_side_semantics() {
        let intersection = intersect_line(
            point(0.0, 0.0),
            point(1.0, 0.0),
            point(2.0, -1.0),
            point(2.0, 1.0),
        )
        .expect("the pinned source accepts intersections beyond the query segment");
        assert_eq!((intersection.x, intersection.y), (2.0, 0.0));
        assert!(
            intersect_line(
                point(0.0, 0.0),
                point(1.0, 0.0),
                point(2.0, 1.0),
                point(2.0, 2.0),
            )
            .is_none()
        );
    }

    #[test]
    fn mermaid12_hourglass_uses_unbiased_polygon_intersection() {
        let node = BoundaryNode {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 30.0,
        };
        for (query_y, expected_y) in [(50.0, 35.0), (-10.0, 5.0)] {
            let intersection = intersect_polygon_hourglass(&node, &point(10.0, query_y));
            assert_eq!((intersection.x, intersection.y), (10.0, expected_y));
        }
    }
}
