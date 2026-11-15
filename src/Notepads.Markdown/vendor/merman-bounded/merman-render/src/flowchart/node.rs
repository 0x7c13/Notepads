// FlowDB supplies no pre-paint width for crossed-circle nodes.
// Pinned crossedCircle.ts therefore captures this radius before updateNodeBounds.
pub(crate) const CROSSED_CIRCLE_RADIUS: f64 = 30.0;

pub(crate) fn flowchart_brace_content_dimensions(
    shape: &str,
    label_width: f64,
    label_height: f64,
    padding: f64,
    look_is_neo: bool,
) -> (f64, f64) {
    // Public Flowchart configuration normalizes negative padding to zero.
    // Mermaid's brace shapes otherwise preserve a zero-sized label box.
    let padding = padding.max(0.0);
    let (padding_x, padding_y) = if look_is_neo {
        if matches!(shape, "comment" | "brace" | "brace-l") {
            (18.0, 12.0)
        } else {
            (36.0, 24.0)
        }
    } else {
        (padding, padding)
    };
    (
        label_width.max(0.0) + padding_x,
        label_height.max(0.0) + padding_y,
    )
}

pub(crate) struct StackedDocumentGeometry {
    pub(crate) outer_points: Vec<(f64, f64)>,
    pub(crate) inner_points: Vec<(f64, f64)>,
    pub(crate) group_dy: f64,
    pub(crate) label_dx: f64,
    pub(crate) label_dy: f64,
}

pub(crate) fn flowchart_stacked_document_geometry(
    label_width: f64,
    label_height: f64,
    padding: f64,
    look_is_neo: bool,
) -> StackedDocumentGeometry {
    // Pinned multiWaveEdgedRectangle.ts: keep the same unshifted outer polygon
    // for updateNodeBounds/intersection and a separate paint-group translation.
    let padding = padding.max(0.0);
    let (padding_x, padding_y) = if look_is_neo {
        (16.0, 12.0)
    } else {
        (padding, padding)
    };
    let w = label_width.max(0.0) + 2.0 * padding_x;
    let h = label_height.max(0.0) + 3.0 * padding_y;
    let amplitude = h / if look_is_neo { 4.0 } else { 8.0 };
    let final_h = h + amplitude / 2.0;
    let x = -w / 2.0;
    let y = -final_h / 2.0;
    let offset = 10.0;
    let wave_x = x - offset;
    let wave_y = y + final_h + offset;
    let delta_x = (x + w - offset) - wave_x;
    let cycle_length = delta_x / 0.8;
    // Preserve the layout's finite zero-span fallback. Upstream divides by
    // zero here for an empty classic label with zero padding.
    let frequency = if cycle_length == 0.0 {
        0.0
    } else {
        2.0 * std::f64::consts::PI / cycle_length
    };
    let mut outer_points = Vec::with_capacity(62);
    outer_points.push((x - offset, y + offset));
    outer_points.push((x - offset, y + final_h + offset));
    for i in 0..=50 {
        let px = wave_x + (i as f64 / 50.0) * delta_x;
        let py = wave_y + amplitude * (frequency * (px - wave_x)).sin();
        outer_points.push((px, py));
    }
    let last_y = outer_points[52].1;
    outer_points.extend([
        (x + w - offset, last_y - offset),
        (x + w, last_y - offset),
        (x + w, last_y - 2.0 * offset),
        (x + w + offset, last_y - 2.0 * offset),
        (x + w + offset, y - offset),
        (x + offset, y - offset),
        (x + offset, y),
        (x, y),
        (x, y + offset),
    ]);
    let inner_points = vec![
        (x, y + offset),
        (x + w - offset, y + offset),
        (x + w - offset, last_y - offset),
        (x + w, last_y - offset),
        (x + w, y),
        (x, y),
    ];
    StackedDocumentGeometry {
        outer_points,
        inner_points,
        group_dy: -amplitude / 2.0,
        label_dx: -offset,
        label_dy: offset - amplitude,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum LeanKind {
    Right,
    Left,
    Trapezoid,
    InvertedTrapezoid,
}

pub(crate) struct LeanGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) points: [(f64, f64); 4],
    pub(crate) translate_x: f64,
    pub(crate) translate_y: f64,
}

impl LeanGeometry {
    pub(crate) fn from_label(
        kind: LeanKind,
        label_width: f64,
        label_height: f64,
        padding: f64,
        neo: bool,
    ) -> Self {
        let padding = padding.max(0.0);
        let scale = if matches!(kind, LeanKind::InvertedTrapezoid) {
            2.0
        } else {
            1.0
        };
        let h = label_height.max(0.0) + padding * scale;
        let w = label_width.max(0.0) + padding * scale * if neo { 2.0 } else { 1.0 };
        Self::from_bounds(kind, w + h, h)
    }

    pub(crate) fn from_bounds(kind: LeanKind, width: f64, height: f64) -> Self {
        let h = height.max(0.0);
        let w = (width - h).max(0.0);
        let dx = 3.0 * h / 6.0;
        let points = match kind {
            LeanKind::Right => [(-dx, 0.0), (w, 0.0), (w + dx, -h), (0.0, -h)],
            LeanKind::Left => [(0.0, 0.0), (w + dx, 0.0), (w, -h), (-dx, -h)],
            LeanKind::Trapezoid => [(-dx, 0.0), (w + dx, 0.0), (w, -h), (0.0, -h)],
            LeanKind::InvertedTrapezoid => [(0.0, 0.0), (w, 0.0), (w + dx, -h), (-dx, -h)],
        };
        Self {
            width: w + h,
            height: h,
            points,
            translate_x: -w / 2.0,
            translate_y: h / 2.0,
        }
    }
}

pub(crate) struct OddGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) points: [(f64, f64); 5],
    pub(crate) shift_x: f64,
}

impl OddGeometry {
    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let p = padding.max(0.0);
        let w = label_width.max(0.0) + if neo { 42.0 } else { p };
        let h = label_height.max(0.0) + if neo { 24.0 } else { p };
        Self::from_bounds(w + h / 4.0, h)
    }

    pub(crate) fn from_bounds(width: f64, height: f64) -> Self {
        let h = height.max(0.0);
        let w = (width - h / 4.0).max(0.0);
        let x = -w / 2.0;
        let y = -h / 2.0;
        let notch = y / 2.0;
        Self {
            width: w + h / 4.0,
            height: h,
            points: [(x + notch, y), (x, 0.0), (x + notch, -y), (-x, -y), (-x, y)],
            shift_x: -notch / 2.0,
        }
    }
}

pub(crate) struct HexagonGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) points: [(f64, f64); 6],
}

impl HexagonGeometry {
    fn shoulder(height: f64, neo: bool) -> f64 {
        height / if neo { 3.5 } else { 4.0 }
    }

    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let p = padding.max(0.0);
        // Pinned hexagon.ts intentionally uses 70 on the height and 32 on the width.
        let h = label_height.max(0.0) + if neo { 70.0 } else { p };
        let w = label_width.max(0.0) + 2.0 * Self::shoulder(h, neo) + if neo { 32.0 } else { p };
        Self::from_bounds(w, h, neo)
    }

    pub(crate) fn from_bounds(width: f64, height: f64, neo: bool) -> Self {
        let w = width.max(0.0);
        let h = height.max(0.0);
        let m = Self::shoulder(h, neo);
        Self {
            width: w,
            height: h,
            points: [
                (m, 0.0),
                (w - m, 0.0),
                (w, -h / 2.0),
                (w - m, -h),
                (m, -h),
                (0.0, -h / 2.0),
            ],
        }
    }
}

pub(crate) struct DoubleCircleGeometry {
    pub(crate) inner_radius: f64,
    pub(crate) outer_radius: f64,
}

impl DoubleCircleGeometry {
    fn gap(neo: bool) -> f64 {
        if neo { 12.0 } else { 5.0 }
    }

    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let w = label_width.max(0.0);
        let h = label_height.max(0.0);
        let inner_radius = (w * w + h * h).sqrt() / 2.0 + if neo { 16.0 } else { padding.max(0.0) };
        Self {
            inner_radius,
            outer_radius: inner_radius + Self::gap(neo),
        }
    }

    pub(crate) fn from_outer_diameter(diameter: f64, neo: bool) -> Self {
        let outer_radius = (diameter / 2.0).max(Self::gap(neo));
        Self {
            outer_radius,
            inner_radius: outer_radius - Self::gap(neo),
        }
    }
}

fn sampled_circle_points(
    center_x: f64,
    center_y: f64,
    radius: f64,
    count: usize,
    start_deg: f64,
    end_deg: f64,
    negate: bool,
) -> Vec<(f64, f64)> {
    let start = start_deg.to_radians();
    let step = (end_deg.to_radians() - start) / (count.saturating_sub(1).max(1) as f64);
    (0..count)
        .map(|i| {
            let angle = start + i as f64 * step;
            let x = center_x + radius * angle.cos();
            let y = center_y + radius * angle.sin();
            if negate { (-x, -y) } else { (x, y) }
        })
        .collect()
}

pub(crate) struct StadiumGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) points: Vec<(f64, f64)>,
}
impl StadiumGeometry {
    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let padding = padding.max(0.0);
        let padding_x = if neo { 40.0 } else { padding };
        let padding_y = if neo { 24.0 } else { padding };
        let h = label_height.max(0.0) + padding_y;
        let inscribed = h * (std::f64::consts::PI / (2.0 * 49.0)).cos();
        let cap = (inscribed * inscribed - label_height.max(0.0).powi(2))
            .max(0.0)
            .sqrt();
        let w = (label_width.max(0.0) + h / 4.0 + padding_x)
            .max(1.5 * h + (h - inscribed))
            .max(label_width.max(0.0) + padding_x + h - cap);
        let radius = h / 2.0;
        let mut points = vec![(-w / 2.0 + radius, -h / 2.0), (w / 2.0 - radius, -h / 2.0)];
        points.extend(sampled_circle_points(
            -w / 2.0 + radius,
            0.0,
            radius,
            50,
            90.0,
            270.0,
            true,
        ));
        points.push((w / 2.0 - radius, h / 2.0));
        points.extend(sampled_circle_points(
            w / 2.0 - radius,
            0.0,
            radius,
            50,
            270.0,
            450.0,
            true,
        ));
        // Keep the source theoretical dimensions. The sampled points are the paint/intersection
        // outline, but using their narrower extrema for layout would shrink the same shape twice.
        Self {
            width: w,
            height: h,
            points,
        }
    }
}

pub(crate) struct DelayGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) points: Vec<(f64, f64)>,
}
impl DelayGeometry {
    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let padding = padding.max(0.0);
        let px = if neo { 16.0 } else { padding };
        let py = if neo { 12.0 } else { padding };
        let min_width = 15.0;
        let min_height = 10.0;
        let h = label_height.max(min_height) + py * 2.0;
        let radius = h / 2.0;
        let cap = radius
            - (radius * radius - (label_height.max(0.0) / 2.0).powi(2))
                .max(0.0)
                .sqrt();
        let w = (label_width.max(min_width) + cap * 2.0) + px * 2.0;
        let mut points = vec![(-w / 2.0, -h / 2.0), (w / 2.0 - radius, -h / 2.0)];
        points.extend(sampled_circle_points(
            -w / 2.0 + radius,
            0.0,
            radius,
            50,
            90.0,
            270.0,
            true,
        ));
        points.extend([(w / 2.0 - radius, h / 2.0), (-w / 2.0, h / 2.0)]);
        Self {
            width: w,
            height: h,
            points,
        }
    }
}

pub(crate) struct DisplayGeometry {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) tx: f64,
    pub(crate) ty: f64,
    pub(crate) points: Vec<(f64, f64)>,
}
impl DisplayGeometry {
    pub(crate) fn from_label(label_width: f64, label_height: f64, padding: f64, neo: bool) -> Self {
        let padding = padding.max(0.0);
        let px = if neo { 16.0 } else { padding };
        let py = if neo { 12.0 } else { padding };
        let min_width = 20.0;
        let min_height = 5.0;
        let h = (label_height.max(0.0) + py * 2.0).max(min_height);
        let radius = h / 2.0;
        let cap = radius
            - (radius * radius - (label_height.max(0.0) / 2.0).powi(2))
                .max(0.0)
                .sqrt();
        let side = (h / 4.0).max(cap);
        let w = (label_width.max(0.0) + px * 2.0 + side * 2.0)
            .max((label_width.max(0.0) + px * 2.0) * 1.25)
            .max(min_width);
        let rw = w - radius;
        let tw = h / 4.0;
        let mut points = vec![(rw, 0.0), (tw, 0.0), (0.0, h / 2.0), (tw, h), (rw, h)];
        points.extend(sampled_circle_points(
            -rw,
            -h / 2.0,
            radius,
            50,
            270.0,
            90.0,
            true,
        ));
        Self {
            width: w,
            height: h,
            tx: -w / 2.0,
            ty: -h / 2.0,
            points,
        }
    }
}

fn node_render_dimensions(
    layout_shape: Option<&str>,
    metrics: crate::text::TextMetrics,
    padding: f64,
    look_is_neo: bool,
) -> (f64, f64) {
    // This function mirrors pinned Mermaid node shape sizing rules at the "rendering-elements"
    // layer, but uses our headless `TextMeasurer` metrics instead of DOM `getBBox()`.
    //
    // References:
    // - `packages/mermaid/src/diagrams/flowchart/flowDb.ts` (shape assignment + padding)
    // - `packages/mermaid/src/rendering-util/rendering-elements/shapes/*.ts` (shape bounds)
    // Mermaid's DOM `getBBox()` can legitimately return 0 for empty/whitespace-only labels.
    // Do not clamp to 1px here, otherwise we skew layout widths (notably `max-width`) by 1px.
    let text_w = metrics.width.max(0.0);
    let text_h = metrics.height.max(0.0);
    let p = padding.max(0.0);

    let shape = layout_shape.unwrap_or("squareRect");
    crate::flowchart::FlowchartShape::resolve(shape)
        .unwrap_or_else(|error| panic!("unvalidated Flowchart shape reached layout: {error}"));

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

    fn bbox_of_points(points: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for &(x, y) in points {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
            Some((min_x, min_y, max_x, max_y))
        } else {
            None
        }
    }

    fn generate_full_sine_wave_points(
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        amplitude: f64,
        num_cycles: f64,
    ) -> Vec<(f64, f64)> {
        // Ported from Mermaid `generateFullSineWavePoints` (50 segments).
        let steps: usize = 50;
        let delta_x = x2 - x1;
        let delta_y = y2 - y1;
        let cycle_length = if num_cycles.abs() < 1e-9 {
            delta_x
        } else {
            delta_x / num_cycles
        };
        let frequency = if cycle_length.abs() < 1e-9 {
            0.0
        } else {
            (2.0 * std::f64::consts::PI) / cycle_length
        };
        let mid_y = y1 + delta_y / 2.0;

        let mut points: Vec<(f64, f64)> = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = (i as f64) / (steps as f64);
            let x = x1 + t * delta_x;
            let y = mid_y + amplitude * (frequency * (x - x1)).sin();
            points.push((x, y));
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
    ) -> Vec<(f64, f64)> {
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
            return vec![(x1, y1), (x2, y2)];
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

        let mut points: Vec<(f64, f64)> = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let t = i as f64 / (num_points - 1) as f64;
            let a = start_angle + t * angle_range;
            let x = center_x + rx * a.cos();
            let y = center_y + ry * a.sin();
            points.push((x, y));
        }
        points
    }

    match shape {
        // Flowchart v2 anchor ignores its label and uses a fixed 1px source radius.
        "anchor" => (2.0, 2.0),

        // Default flowchart process node.
        "squareRect" | "rect" | "proc" | "process" | "rectangle" => {
            if look_is_neo {
                (text_w + 32.0, text_h + 24.0)
            } else {
                (text_w + 4.0 * p, text_h + 2.0 * p)
            }
        }
        "data-store" | "datastore" => (text_w + 4.0 * p, text_h + 2.0 * p),

        // Mermaid uses a few aliases for the same rounded-rectangle shape across layers.
        // In FlowDB output (flowchart-v2), this commonly appears as `rounded`.
        "roundedRect" | "rounded" | "event" => (text_w + 2.0 * p, text_h + 2.0 * p),

        // Note (rendering-elements/state note box).
        "note" => (text_w + 2.0 * p, text_h + 2.0 * p),

        // Diamond (decision/question).
        "diamond" | "question" | "diam" | "decision" => {
            let w = text_w + p;
            let h = text_h + p;
            let s = w + h;
            (s, s)
        }

        // Mermaid 11.17 folder/directory shape: a file-folder outline with a raised tab.
        // The tab is part of the rendered bounds, so include it in the layout height.
        "folder" | "directory" => {
            let w = (text_w + 2.0 * p).max(90.0);
            let content_height = text_h + 2.0 * p;
            let tab_height = (content_height * 0.16).clamp(8.0, 14.0);
            (w, content_height + tab_height)
        }

        // Mermaid 11.17 object-storage bucket: a tapered body plus an elliptical rim.
        "bucket" => {
            let w = (text_w + 2.0 * p).max(80.0);
            let rim_ry = (w * 0.08).clamp(5.0, 12.0);
            (w, text_h + 2.0 * p + rim_ry)
        }

        // Mermaid 11.17 console/browser windows reserve a top chrome band for their decoration.
        "console" => {
            let w = (text_w + 2.0 * p).max(90.0);
            (w, text_h + 2.0 * p + 20.0)
        }
        "browser" => {
            let w = (text_w + 2.0 * p).max(90.0);
            (w, text_h + 2.0 * p + 18.0)
        }

        // Mermaid 11.17 person: a C4-style head above a rounded body.
        "person" => {
            let w = (text_w + 2.0 * p).max(100.0);
            let head_radius = (w * 0.23).clamp(16.0, 56.0);
            let overlap = head_radius * 0.27;
            let body_height = text_h + 2.0 * p;
            (w, body_height + 2.0 * head_radius - overlap)
        }

        "hexagon" | "hex" | "prepare" => {
            let geometry = HexagonGeometry::from_label(text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }

        // Stadium/terminator.
        "stadium" | "terminal" | "pill" => {
            let geometry = StadiumGeometry::from_label(text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }

        // Subroutine/subprocess (framed rectangle): adds an 8px "frame" on both sides.
        "subroutine" | "fr-rect" | "subproc" | "subprocess" | "framed-rectangle" => {
            let w = text_w + if look_is_neo { 28.0 } else { p };
            let h = text_h + if look_is_neo { 12.0 } else { p };
            (w + 16.0, h)
        }

        // Cylinder/database.
        "cylinder" | "cyl" | "db" | "database" => {
            let label_padding = if look_is_neo { 24.0 } else { p };
            let w = text_w + label_padding;
            let rx = w / 2.0;
            let ry = rx / (2.5 + w / 50.0);
            // Mermaid's cylinder path height ends up including two extra `ry` from the ellipses.
            // See `createCylinderPathD` + `translate(..., -(h/2 + ry))`.
            let height = text_h + label_padding + 3.0 * ry;
            (w, height)
        }

        // Flowchart v2 tilted cylinder ("horizontal-cylinder").
        "h-cyl" | "das" | "horizontal-cylinder" => {
            // Mermaid `tiltedCylinder.ts`:
            // - `labelPadding` is 12 for Neo and `halfPadding` for Classic.
            // - `h = bbox.height + labelPadding`
            // - `ry = h / 2`, `rx = ry / (2.5 + h / 50)`
            // - `w = bbox.width + rx + labelPadding`
            // - the rendered `<path>` bbox expands by `rx` on both sides (arc extents), so Dagre
            //   sees `out_w = w + 2*rx` via `updateNodeBounds(...)`.
            let label_padding = if look_is_neo { 12.0 } else { p / 2.0 };
            let h = text_h + label_padding;
            let ry = h / 2.0;
            let rx = if ry == 0.0 {
                0.0
            } else {
                ry / (2.5 + h / 50.0)
            };
            let w = text_w + rx + label_padding;
            (w + 2.0 * rx, h)
        }

        // Flowchart v2 window-pane ("internal-storage").
        "win-pane" | "internal-storage" | "window-pane" => {
            // Mermaid `windowPane.ts` uses fixed 16px/12px axis padding in Neo. The final
            // rendered bbox adds the 10px frame offset on both axes.
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            let rect_offset = 10.0;
            (w + rect_offset, h + rect_offset)
        }

        // Circle. Mermaid sizes the radius from the label bbox diagonal, then adds
        // the look-specific padding (circle.ts: labelRadius + labelPadding/halfPadding).
        // Using the width alone under-sizes empty and wrapped labels, and in neo mode
        // turns a 64px empty circle into a padding-sized dot.
        "circle" | "circ" => {
            let label_diameter = text_w.hypot(text_h);
            let diameter = if look_is_neo {
                label_diameter + 64.0
            } else {
                label_diameter + p
            };
            (diameter, diameter)
        }

        // Organic arc paths use the browser-visible path bbox, not their construction box.
        "bang" => {
            let geometry = crate::flowchart::bang_geometry(text_w, text_h, p);
            (geometry.width(), geometry.height())
        }
        "cloud" => {
            let geometry = crate::flowchart::cloud_geometry(text_w, text_h, p);
            (geometry.width(), geometry.height())
        }

        // Collapsed subgraphs are compact two-row nodes with fixed 8px padding, an 8px
        // separator gap, and a 20px ellipsis row. Mermaid enforces an 80px minimum width.
        "collapsedGroup" | "collapsed-group" => {
            ((text_w + 16.0).max(80.0), text_h + 8.0 + 20.0 + 16.0)
        }

        "doublecircle" | "dbl-circ" | "double-circle" => {
            let geometry = DoubleCircleGeometry::from_label(text_w, text_h, p, look_is_neo);
            let diameter = 2.0 * geometry.outer_radius;
            (diameter, diameter)
        }

        // Small start circle (stateStart in rendering-elements).
        "sm-circ" | "small-circle" | "start" => (14.0, 14.0),

        // Stop framed circle (`stateEnd.ts`) normalizes both source dimensions to 14px.
        "fr-circ" | "framed-circle" | "stop" => (14.0, 14.0),

        // Fork/join bar (uses `lineColor` fill/stroke; no label).
        "fork" | "join" => (70.0, 10.0),

        // Choice diamond (stateChoice in rendering-elements).
        "choice" => (28.0, 28.0),

        // Flowchart v2 lightning bolt (Communication link). Mermaid clears `node.label`.
        "bolt" | "com-link" | "lightning-bolt" => (35.0, 70.0),

        // Flowchart v2 filled circle (`filledCircle.ts`) uses a fixed 7px source radius.
        "f-circ" | "junction" | "filled-circle" => (14.0, 14.0),

        // Flowchart v2 crossed circle (`crossedCircle.ts`) has a minimum 30px source radius.
        "cross-circ" | "summary" | "crossed-circle" => {
            (2.0 * CROSSED_CIRCLE_RADIUS, 2.0 * CROSSED_CIRCLE_RADIUS)
        }

        // Flowchart v2 delay / halfRoundedRectangle.
        "delay" | "half-rounded-rectangle" => {
            let geometry = DelayGeometry::from_label(text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }

        // Flowchart v2 lined cylinder (Disk storage).
        "lin-cyl" | "disk" | "lined-cylinder" => {
            // Mermaid `linedCylinder.ts` uses fixed axis padding for Neo and the configured
            // padding for Classic. The cylinder height includes the top, body, and bottom
            // ellipse radii from the rendered path.
            let (padding_x, padding_y) = if look_is_neo { (16.0, 24.0) } else { (p, p) };
            let w = text_w + 2.0 * padding_x;
            let rx = w / 2.0;
            let ry = rx / (2.5 + w / 50.0);
            let height = text_h + 2.0 * padding_y + 3.0 * ry;
            (w, height)
        }

        // Flowchart v2 curved trapezoid (Display).
        "curv-trap" | "display" | "curved-trapezoid" => {
            let geometry = DisplayGeometry::from_label(text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }

        // Flowchart v2 divided rectangle (Divided process).
        "div-rect" | "div-proc" | "divided-rectangle" | "divided-process" => {
            // Mermaid `dividedRect.ts` uses a single 16px Neo padding term before the
            // rendered 20% top band expands the final height.
            let padding = if look_is_neo { 16.0 } else { p };
            let w = text_w + padding;
            let h = text_h + padding;
            let rect_offset = h * 0.2;
            let x = -w / 2.0;
            let y = -h / 2.0 - rect_offset / 2.0;
            let points: Vec<(f64, f64)> = vec![
                (x, y + rect_offset),
                (-x, y + rect_offset),
                (-x, -y),
                (x, -y),
                (x, y),
                (-x, y),
                (-x, y + rect_offset),
            ];
            let (min_x, min_y, max_x, max_y) = bbox_of_points(&points).unwrap_or((x, y, -x, -y));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 triangle (Extract).
        "tri" | "extract" | "triangle" => {
            // Mermaid uses two-side horizontal padding for Neo and one-side padding for Classic.
            // The same width feeds both layout and SVG geometry.
            let w = text_w + if look_is_neo { 2.0 * p } else { p };
            let h = w + text_h;
            (h, h)
        }

        // Flowchart v2 flipped triangle (Manual file).
        "manual-file" | "flipped-triangle" | "flip-tri" => {
            let w = text_w + if look_is_neo { 2.0 * p } else { p };
            let h = w + text_h;
            (h, h)
        }

        // Flowchart v2 sloped rectangle (Manual input).
        "manual-input" | "sloped-rectangle" | "sl-rect" => {
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            (w, (1.5 * h).max(0.0))
        }

        // Flowchart v2 document (wave-edged rectangle).
        "doc" | "document" => {
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
            let final_h = h + wave_amplitude;
            let min_width = 14.0;
            let extra_w = if w < min_width {
                (min_width - w) / 2.0
            } else {
                0.0
            };

            let mut points: Vec<(f64, f64)> = Vec::new();
            points.push((-w / 2.0 - extra_w, final_h / 2.0));
            points.extend(generate_full_sine_wave_points(
                -w / 2.0 - extra_w,
                final_h / 2.0,
                w / 2.0 + extra_w,
                final_h / 2.0,
                wave_amplitude,
                0.8,
            ));
            points.push((w / 2.0 + extra_w, -final_h / 2.0));
            points.push((-w / 2.0 - extra_w, -final_h / 2.0));

            let (min_x, min_y, max_x, max_y) = bbox_of_points(&points).unwrap_or((
                -w / 2.0,
                -final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
            ));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 stacked document (multi-wave edged rectangle).
        "docs" | "documents" | "st-doc" | "stacked-document" => {
            let geometry =
                flowchart_stacked_document_geometry(text_w, text_h, padding, look_is_neo);
            let (min_x, min_y, max_x, max_y) =
                bbox_of_points(&geometry.outer_points).unwrap_or((0.0, 0.0, 0.0, 0.0));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 stacked rectangle (multi-process).
        "st-rect" | "procs" | "processes" | "stacked-rectangle" => {
            // Mermaid `multiRect.ts` uses fixed Neo label padding and a larger layer offset.
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            let rect_offset = if look_is_neo { 10.0 } else { 5.0 };
            (w + 2.0 * rect_offset, h + 2.0 * rect_offset)
        }

        // Flowchart v2 paper-tape / wave rectangle.
        "paper-tape" | "flag" => {
            // Mermaid `waveRectangle.ts` uses look-specific label padding: Neo reserves
            // 16px horizontally and 20px vertically, while Classic uses the configured padding.
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 20.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + padding_y).max(0.0);
            let wave_amplitude = h / 8.0;
            let final_h = h + wave_amplitude * 2.0;

            let mut points: Vec<(f64, f64)> = Vec::new();
            points.push((-w / 2.0, final_h / 2.0));
            points.extend(generate_full_sine_wave_points(
                -w / 2.0,
                final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
                wave_amplitude,
                1.0,
            ));
            points.push((w / 2.0, -final_h / 2.0));
            points.extend(generate_full_sine_wave_points(
                w / 2.0,
                -final_h / 2.0,
                -w / 2.0,
                -final_h / 2.0,
                wave_amplitude,
                -1.0,
            ));
            let (min_x, min_y, max_x, max_y) = bbox_of_points(&points).unwrap_or((
                -w / 2.0,
                -final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
            ));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 lined document.
        "lin-doc" | "lined-document" => {
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            let wave_amplitude = if look_is_neo { h / 4.0 } else { h / 8.0 };
            let final_h = h + wave_amplitude;
            let extra = (w / 2.0) * 0.1;

            let mut points: Vec<(f64, f64)> = Vec::new();
            points.push((-w / 2.0 - extra, -final_h / 2.0));
            points.push((-w / 2.0 - extra, final_h / 2.0));
            points.extend(generate_full_sine_wave_points(
                -w / 2.0 - extra,
                final_h / 2.0,
                w / 2.0 + extra,
                final_h / 2.0,
                wave_amplitude,
                0.8,
            ));
            points.push((w / 2.0 + extra, -final_h / 2.0));
            points.push((-w / 2.0 - extra, -final_h / 2.0));
            points.push((-w / 2.0, -final_h / 2.0));
            points.push((-w / 2.0, (final_h / 2.0) * 1.1));
            points.push((-w / 2.0, -final_h / 2.0));

            let (min_x, min_y, max_x, max_y) = bbox_of_points(&points).unwrap_or((
                -w / 2.0,
                -final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
            ));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 tagged rectangle.
        "tag-rect" | "tagged-rectangle" | "tag-proc" | "tagged-process" => {
            // Mermaid `taggedRect.ts` applies label padding before adding the tag width.
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = (text_w + 2.0 * padding_x).max(0.0);
            let h = (text_h + 2.0 * padding_y).max(0.0);
            let x = -w / 2.0;
            let y = -h / 2.0;
            let tag_width = 0.2 * h;
            let tag_height = 0.2 * h;
            let rect_points = vec![
                (x - tag_width / 2.0, y),
                (x + w + tag_width / 2.0, y),
                (x + w + tag_width / 2.0, y + h),
                (x - tag_width / 2.0, y + h),
            ];
            let tag_points = vec![
                (x + w - tag_width / 2.0, y + h),
                (x + w + tag_width / 2.0, y + h),
                (x + w + tag_width / 2.0, y + h - tag_height),
            ];
            let mut pts = rect_points;
            pts.extend(tag_points);
            let (min_x, min_y, max_x, max_y) = bbox_of_points(&pts).unwrap_or((x, y, x + w, y + h));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 tagged document.
        "tag-doc" | "tagged-document" => {
            let w = (text_w + 2.0 * p).max(0.0);
            let h = (text_h + 2.0 * p).max(0.0);
            let wave_amplitude = h / 8.0;
            let final_h = h + wave_amplitude;
            let extra = (w / 2.0) * 0.1;
            let tag_width = 0.2 * w;
            let tag_height = 0.2 * h;

            let mut points: Vec<(f64, f64)> = Vec::new();
            points.push((-w / 2.0 - extra, final_h / 2.0));
            points.extend(generate_full_sine_wave_points(
                -w / 2.0 - extra,
                final_h / 2.0,
                w / 2.0 + extra,
                final_h / 2.0,
                wave_amplitude,
                0.8,
            ));
            points.push((w / 2.0 + extra, -final_h / 2.0));
            points.push((-w / 2.0 - extra, -final_h / 2.0));

            let x = -w / 2.0 + extra;
            let y = -final_h / 2.0 - tag_height * 0.4;
            let mut tag_points: Vec<(f64, f64)> = Vec::new();
            tag_points.push((x + w - tag_width, (y + h) * 1.3));
            tag_points.push((x + w, y + h - tag_height));
            tag_points.push((x + w, (y + h) * 0.9));
            tag_points.extend(generate_full_sine_wave_points(
                x + w,
                (y + h) * 1.25,
                x + w - tag_width,
                (y + h) * 1.3,
                -h * 0.02,
                0.5,
            ));

            points.extend(tag_points);
            let (min_x, min_y, max_x, max_y) = bbox_of_points(&points).unwrap_or((
                -w / 2.0,
                -final_h / 2.0,
                w / 2.0,
                final_h / 2.0,
            ));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Flowchart v2 trapezoidal pentagon (Loop limit).
        "notch-pent" | "loop-limit" | "notched-pentagon" => {
            // The source's 15x5 minimum only applies to an explicitly sized node.
            // Flowchart's automatic size comes directly from the measured label.
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            (text_w + 2.0 * padding_x, text_h + 2.0 * padding_y)
        }

        // Flowchart v2 bow-tie rect (Stored data).
        "bow-rect" | "stored-data" | "bow-tie-rectangle" => {
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let w = text_w + 2.0 * padding_x;
            let h = text_h + padding_y;
            let ry = h / 2.0;
            let rx = ry / (2.5 + h / 50.0);
            let mut points: Vec<(f64, f64)> = Vec::new();
            points.push((w / 2.0, -h / 2.0));
            points.push((-w / 2.0, -h / 2.0));
            points.extend(arc_points(
                -w / 2.0,
                -h / 2.0,
                -w / 2.0,
                h / 2.0,
                rx,
                ry,
                false,
            ));
            points.push((w / 2.0, h / 2.0));
            points.extend(arc_points(
                w / 2.0,
                h / 2.0,
                w / 2.0,
                -h / 2.0,
                rx,
                ry,
                true,
            ));
            let (min_x, min_y, max_x, max_y) =
                bbox_of_points(&points).unwrap_or((-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // Hourglass/collate (label cleared, but label group still emitted).
        "hourglass" | "collate" => (30.0, 30.0),

        // Card/notched rectangle: Neo adds 28px horizontally and 24px vertically before drawing the notch.
        "notch-rect" | "notched-rectangle" | "card" => {
            if look_is_neo {
                (text_w + 56.0, text_h + 48.0)
            } else {
                (text_w + p + 12.0, text_h + p)
            }
        }

        // Mermaid `shadedProcess.ts` uses fixed Neo label padding and one 8px frame on
        // the outer width; Classic keeps two frame widths around the configured padding.
        "lin-rect" | "lined-rectangle" | "lined-process" | "lin-proc" | "shaded-process" => {
            let padding_x = if look_is_neo { 16.0 } else { p };
            let padding_y = if look_is_neo { 12.0 } else { p };
            let frame_width = 8.0;
            let frame_count = if look_is_neo { 1.0 } else { 2.0 };
            (
                text_w + 2.0 * padding_x + frame_count * frame_width,
                text_h + 2.0 * padding_y,
            )
        }

        // Text block: bbox + 1x padding (not 2x).
        "text" => (text_w + p, text_h + p),

        // Curly brace comment shapes (rendering-elements).
        "comment" | "brace" | "brace-l" => {
            let (w, h) =
                flowchart_brace_content_dimensions(shape, text_w, text_h, padding, look_is_neo);
            let radius = (h * 0.1).max(5.0);
            let group_tx = radius;
            let mut points: Vec<(f64, f64)> = Vec::new();
            points.extend(circle_points(
                w / 2.0,
                -h / 2.0,
                radius,
                30,
                -90.0,
                0.0,
                true,
            ));
            points.push((-w / 2.0 - radius, radius));
            points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                true,
            ));
            points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ));
            points.push((-w / 2.0 - radius, -h / 2.0));
            points.extend(circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true));

            let mut rect_points: Vec<(f64, f64)> = Vec::new();
            rect_points.extend([(w / 2.0, -h / 2.0 - radius), (-w / 2.0, -h / 2.0 - radius)]);
            rect_points.extend(circle_points(
                w / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                0.0,
                true,
            ));
            rect_points.push((-w / 2.0 - radius, -radius));
            rect_points.extend(circle_points(
                w / 2.0 + w * 0.1,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                true,
            ));
            rect_points.extend(circle_points(
                w / 2.0 + w * 0.1,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ));
            rect_points.push((-w / 2.0 - radius, h / 2.0));
            rect_points.extend(circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true));
            rect_points.extend([(-w / 2.0, h / 2.0 + radius), (w / 2.0, h / 2.0 + radius)]);
            for p in points.iter_mut().chain(rect_points.iter_mut()) {
                p.0 += group_tx;
            }
            let mut all_points: Vec<(f64, f64)> =
                Vec::with_capacity(points.len() + rect_points.len());
            all_points.extend(points);
            all_points.extend(rect_points);
            let (min_x, min_y, max_x, max_y) =
                bbox_of_points(&all_points).unwrap_or((-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }
        "brace-r" => {
            let (w, h) =
                flowchart_brace_content_dimensions(shape, text_w, text_h, padding, look_is_neo);
            let radius = (h * 0.1).max(5.0);
            let group_tx = -radius;
            let mut rect_points: Vec<(f64, f64)> = Vec::new();
            rect_points.extend([(-w / 2.0, -h / 2.0 - radius), (w / 2.0, -h / 2.0 - radius)]);
            rect_points.extend(circle_points(
                w / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                0.0,
                false,
            ));
            rect_points.push((w / 2.0 + radius, -radius));
            rect_points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                false,
            ));
            rect_points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                false,
            ));
            rect_points.push((w / 2.0 + radius, h / 2.0));
            rect_points.extend(circle_points(
                w / 2.0,
                h / 2.0,
                radius,
                20,
                0.0,
                90.0,
                false,
            ));
            rect_points.extend([(w / 2.0, h / 2.0 + radius), (-w / 2.0, h / 2.0 + radius)]);
            for p in &mut rect_points {
                p.0 += group_tx;
            }
            let (min_x, min_y, max_x, max_y) =
                bbox_of_points(&rect_points).unwrap_or((-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }
        "braces" => {
            let (w, h) =
                flowchart_brace_content_dimensions(shape, text_w, text_h, padding, look_is_neo);
            let radius = (h * 0.1).max(5.0);
            let group_tx = radius - radius / 4.0;
            let mut rect_points: Vec<(f64, f64)> = Vec::new();
            rect_points.extend([(w / 2.0, -h / 2.0 - radius), (-w / 2.0, -h / 2.0 - radius)]);
            rect_points.extend(circle_points(
                w / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                0.0,
                true,
            ));
            rect_points.push((-w / 2.0 - radius, -radius));
            rect_points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                -radius,
                radius,
                20,
                -180.0,
                -270.0,
                true,
            ));
            rect_points.extend(circle_points(
                w / 2.0 + radius * 2.0,
                radius,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ));
            rect_points.push((-w / 2.0 - radius, h / 2.0));
            rect_points.extend(circle_points(w / 2.0, h / 2.0, radius, 20, 0.0, 90.0, true));
            rect_points.extend([
                (-w / 2.0, h / 2.0 + radius),
                (w / 2.0 - radius - radius / 2.0, h / 2.0 + radius),
            ]);
            rect_points.extend(circle_points(
                -w / 2.0 + radius + radius / 2.0,
                -h / 2.0,
                radius,
                20,
                -90.0,
                -180.0,
                true,
            ));
            rect_points.push((w / 2.0 - radius / 2.0, radius));
            rect_points.extend(circle_points(
                -w / 2.0 - radius / 2.0,
                -radius,
                radius,
                20,
                0.0,
                90.0,
                true,
            ));
            rect_points.extend(circle_points(
                -w / 2.0 - radius / 2.0,
                radius,
                radius,
                20,
                -90.0,
                0.0,
                true,
            ));
            rect_points.push((w / 2.0 - radius / 2.0, -radius));
            rect_points.extend(circle_points(
                -w / 2.0 + radius + radius / 2.0,
                h / 2.0,
                radius,
                30,
                -180.0,
                -270.0,
                true,
            ));
            for p in &mut rect_points {
                p.0 += group_tx;
            }
            let (min_x, min_y, max_x, max_y) =
                bbox_of_points(&rect_points).unwrap_or((-w / 2.0, -h / 2.0, w / 2.0, h / 2.0));
            ((max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        }

        // The first three variants share dimensions; only their vertex order differs.
        "lean_right" | "lean-r" | "lean-right" | "in-out" | "lean_left" | "lean-l"
        | "lean-left" | "out-in" | "trapezoid" | "trap-b" | "priority" | "trapezoid-bottom" => {
            let geometry =
                LeanGeometry::from_label(LeanKind::Right, text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }
        "inv_trapezoid" | "inv-trapezoid" | "trap-t" | "manual" | "trapezoid-top" => {
            let geometry = LeanGeometry::from_label(
                LeanKind::InvertedTrapezoid,
                text_w,
                text_h,
                p,
                look_is_neo,
            );
            (geometry.width, geometry.height)
        }
        "odd" | "rect_left_inv_arrow" => {
            let geometry = OddGeometry::from_label(text_w, text_h, p, look_is_neo);
            (geometry.width, geometry.height)
        }

        // Ellipses are currently broken upstream but still emitted by FlowDB.
        // Keep a reasonable headless size for layout stability.
        "ellipse" => (text_w + 2.0 * p, text_h + 2.0 * p),

        _ => panic!("Flowchart shape {shape} has no layout geometry"),
    }
}

pub(crate) fn flowchart_node_render_dimensions(
    layout_shape: Option<&str>,
    metrics: crate::text::TextMetrics,
    padding: f64,
    look_is_neo: bool,
) -> (f64, f64) {
    node_render_dimensions(layout_shape, metrics, padding, look_is_neo)
}

/// Shared image geometry for layout and SVG painting. Asset dimensions retain the headless
/// fallback used by the renderer; width constraints follow Mermaid's `imageSquare.ts`.
pub(crate) struct ImageSquareGeometry {
    pub(crate) image_width: f64,
    pub(crate) image_height: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl ImageSquareGeometry {
    pub(crate) fn from_label(
        metrics: crate::text::TextMetrics,
        has_label: bool,
        asset_width: Option<f64>,
        asset_height: Option<f64>,
        constraint_on: bool,
        wrapping_width: f64,
    ) -> Self {
        let asset_h = asset_height.unwrap_or(60.0).max(1.0);
        let asset_w = asset_width.unwrap_or(asset_h).max(1.0);
        let aspect_ratio = asset_w / asset_h;
        let image_raw_width = asset_w.max(if has_label {
            wrapping_width.max(0.0)
        } else {
            0.0
        });
        let image_width = if constraint_on && asset_height.is_some() {
            asset_h * aspect_ratio
        } else {
            image_raw_width
        };
        let image_height = if constraint_on {
            image_width / aspect_ratio
        } else {
            asset_h
        };
        let background_padding = if has_label { 4.0 } else { 0.0 };
        let label_padding = if has_label { 8.0 } else { 0.0 };
        Self {
            image_width,
            image_height,
            width: image_width.max(metrics.width + background_padding),
            height: image_height + metrics.height + background_padding + label_padding,
        }
    }
}

pub(crate) struct NodeLayoutDimensionsRequest<'a> {
    pub(crate) layout_shape: Option<&'a str>,
    pub(crate) layout_direction: &'a str,
    pub(crate) metrics: crate::text::TextMetrics,
    pub(crate) has_label: bool,
    pub(crate) wrapping_width: f64,
    pub(crate) node_constraint: Option<&'a str>,
    pub(crate) padding: f64,
    pub(crate) look_is_neo: bool,
    pub(crate) state_padding: f64,
    pub(crate) node_icon: Option<&'a str>,
    pub(crate) node_img: Option<&'a str>,
    pub(crate) node_pos: Option<&'a str>,
    pub(crate) node_asset_width: Option<f64>,
    pub(crate) node_asset_height: Option<f64>,
}

pub(crate) fn node_layout_dimensions(req: NodeLayoutDimensionsRequest<'_>) -> (f64, f64) {
    let NodeLayoutDimensionsRequest {
        layout_shape,
        layout_direction,
        metrics,
        has_label,
        wrapping_width,
        node_constraint,
        padding,
        look_is_neo,
        state_padding,
        node_icon,
        node_img,
        node_pos,
        node_asset_width,
        node_asset_height,
    } = req;

    let shape = layout_shape.unwrap_or("squareRect");
    let resolved_shape = crate::flowchart::FlowchartShape::resolve(shape)
        .unwrap_or_else(|error| panic!("unvalidated Flowchart shape reached layout: {error}"));

    if resolved_shape == crate::flowchart::FlowchartShape::ImageSquare
        && node_img.is_some_and(|s| !s.trim().is_empty())
    {
        let geometry = ImageSquareGeometry::from_label(
            metrics,
            has_label,
            node_asset_width,
            node_asset_height,
            node_constraint == Some("on"),
            wrapping_width,
        );
        return (geometry.width, geometry.height);
    }

    if matches!(
        resolved_shape,
        crate::flowchart::FlowchartShape::Icon
            | crate::flowchart::FlowchartShape::IconCircle
            | crate::flowchart::FlowchartShape::IconRounded
            | crate::flowchart::FlowchartShape::IconSquare
    ) {
        let label_padding = if has_label { 8.0 } else { 0.0 };
        let label_bbox_w = if has_label { metrics.width + 4.0 } else { 0.0 };
        let label_bbox_h = if has_label { metrics.height + 4.0 } else { 0.0 };

        let asset_h = node_asset_height.unwrap_or(48.0);
        let asset_w = node_asset_width.unwrap_or(48.0);
        let icon_size = asset_h.max(asset_w);
        let has_icon = node_icon.is_some_and(|icon| !icon.trim().is_empty());
        let icon_outer_size = match resolved_shape {
            crate::flowchart::FlowchartShape::IconCircle => {
                let icon_bbox_size = if has_icon { icon_size } else { 0.0 };
                icon_bbox_size * std::f64::consts::SQRT_2 + 40.0
            }
            crate::flowchart::FlowchartShape::IconRounded
            | crate::flowchart::FlowchartShape::IconSquare => icon_size + padding,
            crate::flowchart::FlowchartShape::Icon => icon_size,
            _ => unreachable!("the icon branch excludes non-icon shapes"),
        };

        let outer_w = icon_outer_size.max(label_bbox_w);
        let outer_h = icon_outer_size + label_padding + label_bbox_h;

        // Mermaid icon helpers support `pos=t` for top-aligned labels, but that does not
        // change the node's outer bbox.
        let _ = node_pos;
        return (outer_w, outer_h);
    }

    // Mermaid `forkJoin.ts` inflates the Dagre node dimensions by `state.padding / 2` after
    // `updateNodeBounds(...)`, but does not re-render the rectangle with the inflated size.
    // Horizontal flow (LR or RL) uses a vertical bar, perpendicular to the flow.
    // Keep our layout spacing consistent with upstream by applying both rules here.
    if matches!(shape, "fork" | "join") {
        let extra = (state_padding / 2.0).max(0.0);
        let (render_w, render_h) = if layout_direction.eq_ignore_ascii_case("LR")
            || layout_direction.eq_ignore_ascii_case("RL")
        {
            (10.0, 70.0)
        } else {
            (70.0, 10.0)
        };
        return (render_w + extra, render_h + extra);
    }

    let (render_w, render_h) = node_render_dimensions(Some(shape), metrics, padding, look_is_neo);

    // Mermaid flowchart-v2 renders nodes using the "rendering-elements" layer:
    // 1) it generates SVG paths (roughjs-based even for non-handDrawn look),
    // 2) calls `updateNodeBounds(node, shapeElem)` which sets `node.width/height` from `getBBox()`,
    // 3) then feeds those updated dimensions into Dagre for layout.
    //
    // StadiumGeometry already owns the source dimensions and sampled points. Keep its
    // theoretical width here; re-sampling would shrink the same source geometry twice.

    (render_w, render_h)
}

#[cfg(test)]
mod render_dimension_tests {
    use super::*;

    fn metrics() -> crate::text::TextMetrics {
        crate::text::TextMetrics {
            width: 100.0,
            height: 20.0,
            line_count: 1,
        }
    }

    #[test]
    fn curved_neo_shapes_match_pinned_dimensions_and_sampling() {
        let stadium = StadiumGeometry::from_label(100.0, 20.0, 15.0, false);
        assert_eq!((stadium.width, stadium.height), (123.75, 35.0));
        let neo_stadium = StadiumGeometry::from_label(100.0, 20.0, 15.0, true);
        assert_eq!((neo_stadium.width, neo_stadium.height), (151.0, 44.0));
        assert_eq!(stadium.points.len(), 103);
        assert_eq!(neo_stadium.points.len(), 103);
        let delay = DelayGeometry::from_label(100.0, 20.0, 15.0, false);
        let neo_delay = DelayGeometry::from_label(100.0, 20.0, 15.0, true);
        assert!((delay.width - 134.174243).abs() < 1e-5 && delay.height == 50.0);
        assert!((neo_delay.width - 136.808164).abs() < 1e-5 && neo_delay.height == 44.0);
        let display = DisplayGeometry::from_label(100.0, 20.0, 15.0, false);
        let neo_display = DisplayGeometry::from_label(100.0, 20.0, 15.0, true);
        assert_eq!((display.width, display.height), (162.5, 50.0));
        assert_eq!((neo_display.width, neo_display.height), (165.0, 44.0));
        assert_eq!(display.points.len(), 55);
        assert_eq!(neo_display.points.len(), 55);
        for shape in [
            StadiumGeometry::from_label(0.0, 0.0, 0.0, false).points,
            DelayGeometry::from_label(0.0, 0.0, 0.0, false).points,
            DisplayGeometry::from_label(0.0, 0.0, 0.0, false).points,
        ] {
            assert!(shape.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
        }
        for neo in [false, true] {
            assert_eq!(
                node_render_dimensions(Some("stadium"), metrics(), -2.0, neo),
                node_render_dimensions(Some("stadium"), metrics(), 0.0, neo)
            );
            assert_eq!(
                node_render_dimensions(Some("delay"), metrics(), -2.0, neo),
                node_render_dimensions(Some("delay"), metrics(), 0.0, neo)
            );
            assert_eq!(
                node_render_dimensions(Some("display"), metrics(), -2.0, neo),
                node_render_dimensions(Some("display"), metrics(), 0.0, neo)
            );
        }
    }

    #[test]
    fn neo_shape_dimensions_match_pinned_source() {
        // Executed Mermaid12 shape prefixes, with synthetic label bounds 100x20.
        for (
            neo,
            padding,
            lean_w,
            lean_h,
            inverted_w,
            inverted_h,
            odd_w,
            odd_h,
            hex_w,
            hex_h,
            circle_d,
        ) in [
            (
                false,
                0.0,
                120.0,
                20.0,
                120.0,
                20.0,
                105.0,
                20.0,
                110.0,
                20.0,
                111.9803902718557,
            ),
            (
                false,
                15.0,
                150.0,
                35.0,
                180.0,
                50.0,
                123.75,
                35.0,
                132.5,
                35.0,
                141.9803902718557,
            ),
            (
                false,
                31.0,
                182.0,
                51.0,
                244.0,
                82.0,
                143.75,
                51.0,
                156.5,
                51.0,
                173.9803902718557,
            ),
            (
                true,
                0.0,
                120.0,
                20.0,
                120.0,
                20.0,
                153.0,
                44.0,
                183.42857142857144,
                90.0,
                157.9803902718557,
            ),
            (
                true,
                15.0,
                165.0,
                35.0,
                210.0,
                50.0,
                153.0,
                44.0,
                183.42857142857144,
                90.0,
                157.9803902718557,
            ),
            (
                true,
                31.0,
                213.0,
                51.0,
                306.0,
                82.0,
                153.0,
                44.0,
                183.42857142857144,
                90.0,
                157.9803902718557,
            ),
        ] {
            for (shape, expected) in [
                ("lean_right", (lean_w, lean_h)),
                ("lean_left", (lean_w, lean_h)),
                ("trapezoid", (lean_w, lean_h)),
                ("inv_trapezoid", (inverted_w, inverted_h)),
                ("odd", (odd_w, odd_h)),
                ("hexagon", (hex_w, hex_h)),
                ("doublecircle", (circle_d, circle_d)),
            ] {
                let actual = node_render_dimensions(Some(shape), metrics(), padding, neo);
                assert!(
                    (actual.0 - expected.0).abs() < 1e-9 && (actual.1 - expected.1).abs() < 1e-9,
                    "{shape}, neo={neo}, padding={padding}: {actual:?}, expected {expected:?}"
                );
            }
        }
    }

    #[test]
    fn neo_shape_vertices_preserve_empty_and_tall_labels() {
        for (kind, expected) in [
            (
                LeanKind::Right,
                [(-17.5, 0.0), (130.0, 0.0), (147.5, -35.0), (0.0, -35.0)],
            ),
            (
                LeanKind::Left,
                [(0.0, 0.0), (147.5, 0.0), (130.0, -35.0), (-17.5, -35.0)],
            ),
            (
                LeanKind::Trapezoid,
                [(-17.5, 0.0), (147.5, 0.0), (130.0, -35.0), (0.0, -35.0)],
            ),
            (
                LeanKind::InvertedTrapezoid,
                [(0.0, 0.0), (160.0, 0.0), (185.0, -50.0), (-25.0, -50.0)],
            ),
        ] {
            assert_eq!(
                LeanGeometry::from_label(kind, 100.0, 20.0, 15.0, true).points,
                expected
            );
            let empty = LeanGeometry::from_label(kind, 0.0, 0.0, 0.0, false);
            assert_eq!((empty.width, empty.height), (0.0, 0.0));
            assert!(empty.points.iter().all(|&(x, y)| x == 0.0 && y == 0.0));
        }
        let odd = OddGeometry::from_label(100.0, 20.0, 15.0, true);
        assert_eq!(
            odd.points,
            [
                (-82.0, -22.0),
                (-71.0, 0.0),
                (-82.0, 22.0),
                (71.0, 22.0),
                (71.0, -22.0)
            ]
        );
        assert_eq!(odd.shift_x, 5.5);
        let empty = OddGeometry::from_label(0.0, 0.0, 0.0, true);
        assert_eq!(
            (empty.width, empty.height, empty.shift_x),
            (48.0, 24.0, 3.0)
        );
        let hex = HexagonGeometry::from_label(0.0, 0.0, 0.0, true);
        assert_eq!(
            (hex.width, hex.height, hex.points[0]),
            (72.0, 70.0, (20.0, 0.0))
        );
        let empty = HexagonGeometry::from_label(0.0, 0.0, 0.0, false);
        assert_eq!((empty.width, empty.height), (0.0, 0.0));
        let tall = HexagonGeometry::from_label(20.0, 160.0, 15.0, true);
        assert_eq!(tall.height, 230.0);
        assert!((tall.points[0].0 - 65.71428571428571).abs() < 1e-9);
        for (neo, inner, outer) in [(false, 40.0, 45.0), (true, 41.0, 53.0)] {
            let circle = DoubleCircleGeometry::from_label(40.0, 30.0, 15.0, neo);
            assert_eq!((circle.inner_radius, circle.outer_radius), (inner, outer));
            let painted = DoubleCircleGeometry::from_outer_diameter(2.0 * circle.outer_radius, neo);
            assert_eq!((painted.inner_radius, painted.outer_radius), (inner, outer));
            let tall = DoubleCircleGeometry::from_label(20.0, 160.0, 15.0, neo);
            let expected = if neo {
                108.62257748298549
            } else {
                100.62257748298549
            };
            assert!((tall.outer_radius - expected).abs() < 1e-9);
        }
        let empty = DoubleCircleGeometry::from_label(0.0, 0.0, 0.0, false);
        assert_eq!((empty.inner_radius, empty.outer_radius), (0.0, 5.0));
        let painted = DoubleCircleGeometry::from_outer_diameter(10.0, false);
        assert_eq!(painted.inner_radius, 0.0);
        let empty = DoubleCircleGeometry::from_label(0.0, 0.0, 0.0, true);
        assert_eq!((empty.inner_radius, empty.outer_radius), (16.0, 28.0));
        for shape in [
            "lean_right",
            "lean_left",
            "trapezoid",
            "inv_trapezoid",
            "odd",
            "hexagon",
            "doublecircle",
        ] {
            for neo in [false, true] {
                assert_eq!(
                    node_render_dimensions(Some(shape), metrics(), -2.0, neo),
                    node_render_dimensions(Some(shape), metrics(), 0.0, neo)
                );
            }
        }
    }

    #[test]
    fn stacked_document_geometry_matches_pinned_neo_and_classic_source() {
        // Captured from the pinned TS outerPathPoints using its sine generator.
        for (neo, padding, width, height, bounds, label_dy, group_dy, last_wave_y) in [
            (
                true,
                15.0,
                100.0,
                20.0,
                (-76.0, -41.5, 76.0, 55.49005261696825),
                -4.0,
                -7.0,
                28.18520877186785,
            ),
            (
                false,
                15.0,
                100.0,
                20.0,
                (-75.0, -44.53125, 75.0, 52.65047696520479),
                1.875,
                -4.0625,
                36.80391580510188,
            ),
            (
                false,
                0.0,
                100.0,
                20.0,
                (-60.0, -20.625, 60.0, 23.123223681601473),
                7.5,
                -1.25,
                18.247358709262116,
            ),
            (
                true,
                0.0,
                0.0,
                0.0,
                (-26.0, -30.25, 26.0, 39.2436052537653),
                1.0,
                -4.5,
                21.690491353343617,
            ),
        ] {
            let geometry = flowchart_stacked_document_geometry(width, height, padding, neo);
            assert_eq!(geometry.outer_points.len(), 62);
            assert_eq!(geometry.inner_points.len(), 6);
            let actual = geometry.outer_points.iter().fold(
                (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ),
                |(min_x, min_y, max_x, max_y), &(x, y)| {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                },
            );
            for (a, b) in [
                (actual.0, bounds.0),
                (actual.1, bounds.1),
                (actual.2, bounds.2),
                (actual.3, bounds.3),
            ] {
                assert!(
                    (a - b).abs() < 1e-9,
                    "neo={neo}, padding={padding}: {actual:?}"
                );
            }
            assert_eq!(geometry.label_dx, -10.0);
            assert_eq!(geometry.label_dy, label_dy);
            assert_eq!(geometry.group_dy, group_dy);
            assert!((geometry.outer_points[52].1 - last_wave_y).abs() < 1e-9);
            for shape in ["docs", "documents", "st-doc", "stacked-document"] {
                let size = node_render_dimensions(
                    Some(shape),
                    crate::text::TextMetrics {
                        width,
                        height,
                        line_count: 1,
                    },
                    padding,
                    neo,
                );
                assert!((size.0 - (bounds.2 - bounds.0)).abs() < 1e-9);
                assert!((size.1 - (bounds.3 - bounds.1)).abs() < 1e-9);
            }
        }
        let neo = flowchart_stacked_document_geometry(100.0, 20.0, 15.0, true);
        for padding in [0.0, -2.0, 100.0] {
            let other = flowchart_stacked_document_geometry(100.0, 20.0, padding, true);
            assert_eq!(other.outer_points, neo.outer_points);
            assert_eq!(other.label_dy, neo.label_dy);
        }
        // Preserve the established finite zero-span layout fallback, rather
        // than importing the TS sine helper's division-by-zero NaNs.
        for padding in [0.0, -2.0] {
            let empty = flowchart_stacked_document_geometry(0.0, 0.0, padding, false);
            assert!(
                empty
                    .outer_points
                    .iter()
                    .chain(&empty.inner_points)
                    .all(|(x, y)| x.is_finite() && y.is_finite())
            );
            assert_eq!(empty.outer_points[0], (-10.0, 10.0));
            assert_eq!(empty.label_dy, 10.0);
            let size = node_render_dimensions(
                Some("documents"),
                crate::text::TextMetrics {
                    width: 0.0,
                    height: 0.0,
                    line_count: 0,
                },
                padding,
                false,
            );
            assert_eq!(size, (20.0, 20.0));
        }
    }

    #[test]
    fn brace_layout_uses_pinned_neo_axis_padding_and_preserves_zero() {
        let empty = crate::text::TextMetrics {
            width: 0.0,
            height: 0.0,
            line_count: 1,
        };
        for (shape, zero_width, neo_width, neo_height) in [
            ("brace", 15.0, 129.8, 42.0),
            ("brace-l", 15.0, 129.8, 42.0),
            ("comment", 15.0, 129.8, 42.0),
            ("brace-r", 10.0, 146.0, 54.0),
            ("braces", 12.5, 148.5, 54.0),
        ] {
            for padding in [0.0, -2.0] {
                let zero = node_render_dimensions(Some(shape), empty, padding, false);
                assert!((zero.0 - zero_width).abs() < 1e-9, "{shape}: {zero:?}");
                assert!((zero.1 - 10.0).abs() < 1e-9, "{shape}: {zero:?}");
            }
            // Neo owns its axis padding, independent of configured padding.
            for padding in [0.0, 15.0, 40.0] {
                let neo = node_render_dimensions(Some(shape), metrics(), padding, true);
                assert!((neo.0 - neo_width).abs() < 1e-9, "{shape}: {neo:?}");
                assert!((neo.1 - neo_height).abs() < 1e-9, "{shape}: {neo:?}");
            }
        }
    }

    #[test]
    fn neo_uses_pinned_shape_specific_content_padding() {
        assert_eq!(
            node_render_dimensions(Some("squareRect"), metrics(), 15.0, true),
            (132.0, 44.0)
        );
        assert_eq!(
            node_render_dimensions(Some("stadium"), metrics(), 15.0, true),
            (151.0, 44.0)
        );
        assert_eq!(
            node_render_dimensions(Some("subroutine"), metrics(), 15.0, true),
            (144.0, 32.0)
        );

        let (cylinder_w, cylinder_h) =
            node_render_dimensions(Some("cylinder"), metrics(), 15.0, true);
        let expected_ry = 62.0 / (2.5 + 124.0 / 50.0);
        assert_eq!(cylinder_w, 124.0);
        assert!((cylinder_h - (44.0 + 3.0 * expected_ry)).abs() < 1e-9);

        // `linedCylinder.ts` uses independent fixed x/y padding in Neo mode.
        for (neo, padding_x, padding_y) in [(false, 15.0, 15.0), (true, 16.0, 24.0)] {
            let (width, height) =
                node_render_dimensions(Some("lined-cylinder"), metrics(), 15.0, neo);
            let expected_width = 100.0 + 2.0 * padding_x;
            let rx = expected_width / 2.0;
            let ry = rx / (2.5 + expected_width / 50.0);
            let expected_height = 20.0 + 2.0 * padding_y + 3.0 * ry;
            assert!(
                (width - expected_width).abs() < 1e-9,
                "{neo}: width={width}"
            );
            assert!(
                (height - expected_height).abs() < 1e-9,
                "{neo}: height={height}"
            );
        }
        assert_eq!(
            node_render_dimensions(Some("triangle"), metrics(), 15.0, true),
            (150.0, 150.0)
        );
        assert_eq!(
            node_render_dimensions(Some("flipped-triangle"), metrics(), 15.0, true),
            (150.0, 150.0)
        );
        assert_eq!(
            node_render_dimensions(Some("triangle"), metrics(), 15.0, false),
            (135.0, 135.0)
        );
        assert_eq!(
            node_render_dimensions(Some("flipped-triangle"), metrics(), 15.0, false),
            (135.0, 135.0)
        );
        let classic_h_cyl = node_render_dimensions(Some("h-cyl"), metrics(), 15.0, false);
        let classic_h = 20.0 + 7.5;
        let classic_ry = classic_h / 2.0;
        let classic_rx = classic_ry / (2.5 + classic_h / 50.0);
        assert!((classic_h_cyl.0 - (100.0 + 7.5 + 3.0 * classic_rx)).abs() < 1e-9);
        assert!((classic_h_cyl.1 - classic_h).abs() < 1e-9);
        let neo_h_cyl = node_render_dimensions(Some("h-cyl"), metrics(), 15.0, true);
        let neo_h = 20.0 + 12.0;
        let neo_ry = neo_h / 2.0;
        let neo_rx = neo_ry / (2.5 + neo_h / 50.0);
        assert!((neo_h_cyl.0 - (100.0 + 12.0 + 3.0 * neo_rx)).abs() < 1e-9);
        assert!((neo_h_cyl.1 - neo_h).abs() < 1e-9);
        assert_eq!(
            node_render_dimensions(Some("document"), metrics(), 15.0, false),
            (130.0, 62.49555920400368)
        );
        assert_eq!(
            node_render_dimensions(Some("document"), metrics(), 15.0, true),
            (132.0, 65.99218419904648)
        );
        assert_eq!(
            node_render_dimensions(Some("sloped-rectangle"), metrics(), 15.0, false),
            (130.0, 75.0)
        );
        assert_eq!(
            node_render_dimensions(Some("sloped-rectangle"), metrics(), 15.0, true),
            (132.0, 66.0)
        );
        assert_eq!(
            node_render_dimensions(Some("card"), metrics(), 15.0, false),
            (127.0, 35.0)
        );
        assert_eq!(
            node_render_dimensions(Some("card"), metrics(), 15.0, true),
            (156.0, 68.0)
        );

        for (neo, padding_x, padding_y) in [(false, 15.0, 15.0), (true, 16.0, 20.0)] {
            let (width, height) = node_render_dimensions(Some("paper-tape"), metrics(), 15.0, neo);
            let base_w = 100.0 + 2.0 * padding_x;
            let base_h = 20.0 + padding_y;
            let amplitude = base_h / 8.0;
            let final_h = base_h + 2.0 * amplitude;
            let sampled_peak = (0..=50)
                .map(|i| ((i as f64) / 50.0 * std::f64::consts::TAU).sin().abs())
                .fold(0.0, f64::max);
            let expected_h = final_h + 2.0 * amplitude * sampled_peak;
            assert!(
                (width - base_w).abs() < 1e-9,
                "paper tape neo={neo}: {width}"
            );
            assert!(
                (height - expected_h).abs() < 1e-9,
                "paper tape neo={neo}: {height}"
            );
        }

        for (neo, padding_x, padding_y) in [(false, 15.0, 15.0), (true, 16.0, 12.0)] {
            let (width, height) = node_render_dimensions(Some("tag-rect"), metrics(), 15.0, neo);
            let base_h = 20.0 + 2.0 * padding_y;
            let expected_width = 100.0 + 2.0 * padding_x + 0.2 * base_h;
            assert!(
                (width - expected_width).abs() < 1e-9,
                "tag rect neo={neo}: {width}"
            );
            assert!(
                (height - base_h).abs() < 1e-9,
                "tag rect neo={neo}: {height}"
            );
        }

        for (neo, padding_x, padding_y, offset) in
            [(false, 15.0, 15.0, 5.0), (true, 16.0, 12.0, 10.0)]
        {
            let actual = node_render_dimensions(Some("stacked-rectangle"), metrics(), 15.0, neo);
            let expected = (
                100.0 + 2.0 * padding_x + 2.0 * offset,
                20.0 + 2.0 * padding_y + 2.0 * offset,
            );
            assert!(
                (actual.0 - expected.0).abs() < 1e-9,
                "stacked rectangle neo={neo}: {actual:?}"
            );
            assert!(
                (actual.1 - expected.1).abs() < 1e-9,
                "stacked rectangle neo={neo}: {actual:?}"
            );
        }
        // The trapezoidal pentagon has no automatic minimum; 15x5 is only applied
        // by Mermaid when an explicit node width or height is supplied.
        assert_eq!(
            node_render_dimensions(Some("notched-pentagon"), metrics(), 15.0, false),
            (130.0, 50.0)
        );
        assert_eq!(
            node_render_dimensions(Some("notched-pentagon"), metrics(), 15.0, true),
            (132.0, 44.0)
        );
        assert_eq!(
            node_render_dimensions(
                Some("notched-pentagon"),
                crate::text::TextMetrics {
                    width: 0.0,
                    height: 0.0,
                    line_count: 0,
                },
                0.0,
                false,
            ),
            (0.0, 0.0)
        );
        assert_eq!(
            node_render_dimensions(
                Some("notched-pentagon"),
                crate::text::TextMetrics {
                    width: 0.0,
                    height: 0.0,
                    line_count: 0,
                },
                0.0,
                true,
            ),
            (32.0, 24.0)
        );

        assert_eq!(
            node_render_dimensions(Some("shaded-process"), metrics(), 15.0, false),
            (146.0, 50.0)
        );
        assert_eq!(
            node_render_dimensions(Some("shaded-process"), metrics(), 15.0, true),
            (140.0, 44.0)
        );

        // Stored data uses Neo's 16px/12px axis padding before the sampled arc bbox.
        let neo_bow = node_render_dimensions(Some("bow-tie-rectangle"), metrics(), 15.0, true);
        assert!((neo_bow.0 - 137.07813754398302).abs() < 1e-9);
        assert!((neo_bow.1 - 32.0).abs() < 1e-9);
        let classic_bow = node_render_dimensions(Some("bow-tie-rectangle"), metrics(), 15.0, false);
        assert!((classic_bow.0 - 135.4500714461302).abs() < 1e-9);
        assert!((classic_bow.1 - 35.0).abs() < 1e-9);

        // Window pane: Neo uses 16px horizontal and 12px vertical padding before the
        // fixed 10px frame offset; Classic keeps the configured padding on both axes.
        assert_eq!(
            node_render_dimensions(Some("window-pane"), metrics(), 15.0, false),
            (140.0, 60.0)
        );
        assert_eq!(
            node_render_dimensions(Some("window-pane"), metrics(), 15.0, true),
            (142.0, 54.0)
        );

        // Divided rectangle: the source adds one 16px Neo padding term to each label
        // axis, then the polygon's 20% top band expands only the rendered height.
        assert_eq!(
            node_render_dimensions(Some("divided-rectangle"), metrics(), 15.0, false),
            (115.0, 42.0)
        );
        let divided_neo = node_render_dimensions(Some("divided-rectangle"), metrics(), 15.0, true);
        assert!((divided_neo.0 - 116.0).abs() < 1e-9);
        assert!((divided_neo.1 - 43.2).abs() < 1e-9);
    }

    #[test]
    fn circle_uses_label_diagonal_and_neo_padding() {
        let empty = crate::text::TextMetrics {
            width: 0.0,
            height: 0.0,
            line_count: 1,
        };
        assert_eq!(
            node_render_dimensions(Some("circle"), empty, 15.0, true),
            (64.0, 64.0)
        );

        let value = node_render_dimensions(Some("circle"), metrics(), 15.0, true);
        let expected = 100.0_f64.hypot(20.0) + 64.0;
        assert!((value.0 - expected).abs() < 1e-9);
        assert_eq!(value.0, value.1);

        let classic = node_render_dimensions(Some("circle"), metrics(), 15.0, false);
        let classic_expected = 100.0_f64.hypot(20.0) + 15.0;
        assert!((classic.0 - classic_expected).abs() < 1e-9);
        assert_eq!(
            node_render_dimensions(Some("circle"), empty, 24.0, false),
            (24.0, 24.0)
        );
    }

    #[test]
    fn mermaid_1172_object_shapes_use_pinned_dimensions() {
        let (person_w, person_h) = node_render_dimensions(Some("person"), metrics(), 15.0, false);
        let person_width = 100.0 + 2.0 * 15.0;
        let head_radius = person_width * 0.23;
        assert_eq!(person_w, person_width);
        assert!((person_h - (50.0 + 2.0 * head_radius - head_radius * 0.27)).abs() < 1e-9);
        assert_eq!(
            node_render_dimensions(Some("bucket"), metrics(), 15.0, false),
            (130.0, 60.4)
        );
        assert_eq!(
            node_render_dimensions(Some("console"), metrics(), 15.0, false),
            (130.0, 70.0)
        );
        assert_eq!(
            node_render_dimensions(Some("browser"), metrics(), 15.0, false),
            (130.0, 68.0)
        );
    }

    #[test]
    fn classic_shape_padding_remains_unchanged() {
        assert_eq!(
            node_render_dimensions(Some("squareRect"), metrics(), 15.0, false),
            (160.0, 50.0)
        );
        assert_eq!(
            node_render_dimensions(Some("stadium"), metrics(), 15.0, false),
            (123.75, 35.0)
        );
        assert_eq!(
            node_render_dimensions(Some("subroutine"), metrics(), 15.0, false),
            (131.0, 35.0)
        );
    }
}
