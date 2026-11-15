use super::super::roughjs_common::{ops_to_svg_path_d, parse_hex_color_to_srgba};
use super::super::*;

fn class_parse_stroke_dash_pair(stroke_dasharray: &str) -> (f64, f64) {
    let dash = stroke_dasharray.trim().replace(',', " ");
    let mut nums = dash
        .split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok());
    match (nums.next(), nums.next()) {
        (Some(a), Some(b)) => (a, b),
        (Some(a), None) => (a, a),
        _ => (0.0, 0.0),
    }
}

fn roughjs46_diverge_point(options: &mut roughr::core::Options) -> f64 {
    0.2 + options.random() * 0.2
}

pub(super) fn class_rough_seed(
    base_randomness: &roughr::core::RoughRandomness,
    _diagram_id: &str,
    _dom_id: &str,
) -> roughr::core::RoughRandomness {
    base_randomness.clone()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn class_rough_hachure_rect_paths(
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    fill: &str,
    stroke: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    randomness: &roughr::core::RoughRandomness,
) -> Option<(String, String)> {
    let fill =
        parse_hex_color_to_srgba(fill).unwrap_or_else(|| roughr::Srgba::new(0.0, 0.0, 0.0, 1.0));
    let stroke =
        parse_hex_color_to_srgba(stroke).unwrap_or_else(|| roughr::Srgba::new(0.0, 0.0, 0.0, 1.0));
    let (dash0, dash1) = class_parse_stroke_dash_pair(stroke_dasharray);
    let options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.7)
        .fill(fill)
        .fill_style(roughr::core::FillStyle::Hachure)
        // Mermaid userNodeOverrides sets fill weight and hachure gap independently of outline width.
        .fill_weight(1.5)
        .hachure_gap(1.5)
        .stroke(stroke)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    let generator = roughr::generator::Generator::default();
    let drawable = generator.rectangle::<f64>(left, top, width, height, &Some(options));
    let mut fill_d = None;
    let mut stroke_d = None;

    for set in drawable.sets {
        let d = ops_to_svg_path_d(&set);
        match set.op_set_type {
            roughr::core::OpSetType::FillPath | roughr::core::OpSetType::FillSketch => {
                fill_d = Some(d);
            }
            roughr::core::OpSetType::Path => {
                stroke_d = Some(d);
            }
        }
    }

    Some((fill_d?, stroke_d?))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn class_rough_hand_drawn_line_path(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    randomness: &roughr::core::RoughRandomness,
) -> Option<String> {
    let stroke =
        parse_hex_color_to_srgba(stroke).unwrap_or_else(|| roughr::Srgba::new(0.0, 0.0, 0.0, 1.0));
    let (dash0, dash1) = class_parse_stroke_dash_pair(stroke_dasharray);
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.7)
        .stroke(stroke)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .build()
        .ok()?;

    Some(ops_to_svg_path_d(&roughr::renderer::line::<f64>(
        x1,
        y1,
        x2,
        y2,
        &mut options,
    )))
}

pub(super) fn class_rough_hand_drawn_stroke_path_for_svg_path(
    svg_path_data: &str,
    roughness: f32,
    randomness: &roughr::core::RoughRandomness,
) -> Option<String> {
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(roughness)
        .disable_multi_stroke(false)
        .build()
        .ok()?;

    let opset = roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut options);
    Some(ops_to_svg_path_d(&opset))
}

pub(super) fn class_rough_line_double_path_and_bounds(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    randomness: &roughr::core::RoughRandomness,
) -> (String, super::super::path_bounds::SvgPathBounds) {
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .build()
        .expect("class rough line options must be valid");
    class_rough_line_double_path_and_bounds_with_options(x1, y1, x2, y2, &mut options)
}

fn class_rough_line_double_path_and_bounds_with_options(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    options: &mut roughr::core::Options,
) -> (String, super::super::path_bounds::SvgPathBounds) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let mut d = String::new();
    let mut pb = super::super::path_bounds::SvgPathBounds {
        min_x: x1,
        min_y: y1,
        max_x: x1,
        max_y: y1,
    };

    for idx in 0..2 {
        let diverge = roughjs46_diverge_point(options);
        for _ in 0..10 {
            let _ = options.random();
        }

        let c1x = x1 + dx * diverge;
        let c1y = y1 + dy * diverge;
        let c2x = x1 + dx * 2.0 * diverge;
        let c2y = y1 + dy * 2.0 * diverge;
        if idx > 0 {
            d.push(' ');
        }
        let _ = write!(
            &mut d,
            "M{} {} C{} {}, {} {}, {} {}",
            fmt(x1),
            fmt(y1),
            fmt(c1x),
            fmt(c1y),
            fmt(c2x),
            fmt(c2y),
            fmt(x2),
            fmt(y2),
        );
        super::super::path_bounds::svg_path_bounds_include_cubic(
            &mut pb,
            super::super::path_bounds::CubicBezier {
                x0: x1,
                y0: y1,
                x1: c1x,
                y1: c1y,
                x2: c2x,
                y2: c2y,
                x3: x2,
                y3: y2,
            },
        );
    }

    (d, pb)
}

pub(super) fn class_rough_rect_stroke_path_and_bounds(
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    randomness: &roughr::core::RoughRandomness,
) -> (String, super::super::path_bounds::SvgPathBounds) {
    let right = left + width;
    let bottom = top + height;
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .build()
        .expect("class rough rectangle options must be valid");
    let mut out = String::new();
    let mut pb_opt: Option<super::super::path_bounds::SvgPathBounds> = None;

    for (idx, (ax, ay, bx, by)) in [
        (left, top, right, top),
        (right, top, right, bottom),
        (right, bottom, left, bottom),
        (left, bottom, left, top),
    ]
    .into_iter()
    .enumerate()
    {
        let (segment, seg_pb) =
            class_rough_line_double_path_and_bounds_with_options(ax, ay, bx, by, &mut options);
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(&segment);
        if let Some(pb) = pb_opt.as_mut() {
            pb.min_x = pb.min_x.min(seg_pb.min_x);
            pb.min_y = pb.min_y.min(seg_pb.min_y);
            pb.max_x = pb.max_x.max(seg_pb.max_x);
            pb.max_y = pb.max_y.max(seg_pb.max_y);
        } else {
            pb_opt = Some(seg_pb);
        }
    }

    (
        out,
        pb_opt.unwrap_or(super::super::path_bounds::SvgPathBounds {
            min_x: left,
            min_y: top,
            max_x: right,
            max_y: bottom,
        }),
    )
}
