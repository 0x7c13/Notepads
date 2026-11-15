//! RoughJS-compatible path generation helpers (via `roughr`).
//!
//! Mermaid uses RoughJS for "hand-drawn" flowchart node rendering, and in a few cases even when
//! roughness is zero. These helpers mirror Mermaid's RoughJS call ordering to keep SVG DOM parity.

use crate::svg::parity::roughjs_common::ops_to_svg_path_d;
use roughr::core::RoughRandomness;

pub(in crate::svg::parity) use crate::svg::parity::roughjs_common::{
    RoughRectSpec, roughjs_circle_path_d, roughjs_paths_for_rect,
};

fn parse_stroke_dash_pair(stroke_dasharray: &str) -> (f64, f64) {
    let dash = stroke_dasharray.trim().replace(',', " ");
    let mut nums = dash
        .split_whitespace()
        .filter_map(|t| t.parse::<f32>().ok());
    match (nums.next(), nums.next()) {
        (Some(a), Some(b)) => (a as f64, b as f64),
        (Some(a), None) => (a as f64, a as f64),
        _ => (0.0, 0.0),
    }
}

pub(in crate::svg::parity) fn roughjs_paths_for_svg_path(
    svg_path_data: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let base_options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.0)
        .bowing(1.0)
        .fill_style(roughr::core::FillStyle::Solid)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    // Rough.js `generator.path(...)`:
    // - `sets = pointsOnPath(d, 1, distance)`
    // - for solid fill, if `sets.length === 1`: fill path from `svgPath(...)` with
    //   `disableMultiStroke: true`, then drop subsequent `move` ops (`_mergedShape`).
    // - otherwise for solid fill: `solidFillPolygon(sets, o)`
    let distance = (1.0 + base_options.roughness.unwrap_or(1.0) as f64) / 2.0;
    let sets = roughr::points_on_path::points_on_path::<f64>(
        svg_path_data.to_string(),
        Some(1.0),
        Some(distance),
    );

    // Rough.js `generator.path(...)` builds the stroke opset first (`shape = svgPath(d, o)`),
    // which initializes and advances `o.randomizer`. For the solid-fill special-case
    // (`sets.length === 1`), it then calls `svgPath(d, Object.assign({}, o, ...))`, which
    // copies the *existing* `randomizer` by reference and therefore continues the PRNG stream.
    //
    // In headless Rust we model that by emitting the stroke opset first (advancing the
    // in-options PRNG state), then cloning the mutated options for the fill pass.
    let mut stroke_opts = base_options.clone();
    let stroke_opset =
        roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut stroke_opts);

    let fill_opset = if sets.len() == 1 {
        let mut fill_opts = stroke_opts.clone();
        fill_opts.disable_multi_stroke = Some(true);
        let base_rough = fill_opts.roughness.unwrap_or(1.0);
        fill_opts.roughness = Some(if base_rough != 0.0 {
            base_rough + 0.8
        } else {
            0.0
        });

        let mut opset =
            roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut fill_opts);
        opset.ops = opset
            .ops
            .iter()
            .cloned()
            .enumerate()
            .filter_map(|(idx, op)| {
                if idx != 0 && op.op == roughr::core::OpType::Move {
                    return None;
                }
                Some(op)
            })
            .collect();
        opset
    } else {
        let mut fill_opts = stroke_opts.clone();
        roughr::renderer::solid_fill_polygon(&sets, &mut fill_opts)
    };

    Some((
        ops_to_svg_path_d(&fill_opset),
        ops_to_svg_path_d(&stroke_opset),
    ))
}

pub(in crate::svg::parity) fn roughjs_paths_for_svg_path_single_set(
    svg_path_data: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    // Variant of `roughjs_paths_for_svg_path(...)` that always takes RoughJS' `sets.length === 1`
    // branch (fill path via `svgPath(...)` with `disableMultiStroke=true`), avoiding the
    // `pointsOnPath(...)` step which can overflow on complex paths.
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let base_options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.0)
        .bowing(1.0)
        .fill_style(roughr::core::FillStyle::Solid)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    // Keep call ordering consistent with RoughJS: stroke pass advances the PRNG before fill.
    let mut stroke_opts = base_options.clone();
    let stroke_opset =
        roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut stroke_opts);

    let mut fill_opts = stroke_opts.clone();
    fill_opts.disable_multi_stroke = Some(true);
    let base_rough = fill_opts.roughness.unwrap_or(1.0);
    fill_opts.roughness = Some(if base_rough != 0.0 {
        base_rough + 0.8
    } else {
        0.0
    });
    let mut fill_opset =
        roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut fill_opts);
    fill_opset.ops = fill_opset
        .ops
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(idx, op)| {
            if idx != 0 && op.op == roughr::core::OpType::Move {
                return None;
            }
            Some(op)
        })
        .collect();

    Some((
        ops_to_svg_path_d(&fill_opset),
        ops_to_svg_path_d(&stroke_opset),
    ))
}

pub(in crate::svg::parity) fn roughjs_stroke_path_for_svg_path(
    svg_path_data: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    randomness: &RoughRandomness,
) -> Option<String> {
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.0)
        .bowing(1.0)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .build()
        .ok()?;

    let opset = roughr::renderer::svg_path::<f64>(svg_path_data.to_string(), &mut options);
    Some(ops_to_svg_path_d(&opset))
}

pub(in crate::svg::parity) fn roughjs_hand_drawn_stroke_path_for_svg_path(
    svg_path_data: &str,
    roughness: f32,
    randomness: &RoughRandomness,
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

#[allow(clippy::too_many_arguments)]
pub(in crate::svg::parity) fn roughjs_hachure_paths_for_svg_path(
    svg_path_data: &str,
    stroke_width: f32,
    stroke_dasharray: &str,
    fill_weight: f32,
    hachure_gap: f32,
    roughness: f32,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(roughness)
        .fill_style(roughr::core::FillStyle::Hachure)
        .fill_weight(fill_weight)
        .hachure_gap(hachure_gap)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    let sets = roughr::points_on_path::points_on_path::<f64>(
        svg_path_data.to_owned(),
        Some(1.0),
        Some((1.0 + roughness as f64) / 2.0),
    );
    // Generator.path generates the fill first, then the outline on the same random stream.
    let fill = roughr::renderer::pattern_fill_polygons(sets, &mut options);
    let stroke = roughr::renderer::svg_path(svg_path_data.to_owned(), &mut options);
    Some((ops_to_svg_path_d(&fill), ops_to_svg_path_d(&stroke)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::svg::parity) fn roughjs_hachure_paths_for_rect(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    stroke_width: f32,
    stroke_dasharray: &str,
    fill_weight: f32,
    hachure_gap: f32,
    roughness: f32,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(roughness)
        .fill_style(roughr::core::FillStyle::Hachure)
        .fill_weight(fill_weight)
        .hachure_gap(hachure_gap)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    // Generator.rectangle generates the outline before the hachure fill.
    let stroke = roughr::renderer::rectangle::<f64>(x, y, w, h, &mut options);
    let points = vec![
        roughr::Point2D::new(x, y),
        roughr::Point2D::new(x + w, y),
        roughr::Point2D::new(x + w, y + h),
        roughr::Point2D::new(x, y + h),
    ];
    let fill = roughr::renderer::pattern_fill_polygons(vec![points], &mut options);
    Some((ops_to_svg_path_d(&fill), ops_to_svg_path_d(&stroke)))
}

pub(in crate::svg::parity) fn roughjs_paths_for_polygon(
    points: &[(f64, f64)],
    stroke_width: f32,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    // Mirror RoughJS `generator.polygon(...)` generation order: outline first, then fill, then
    // emit fill before outline.
    let mut opts = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(0.0)
        .fill_style(roughr::core::FillStyle::Solid)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![0.0, 0.0])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    let pts: Vec<_> = points
        .iter()
        .copied()
        .map(|(x, y)| roughr::Point2D::new(x, y))
        .collect();
    let outline_opset = roughr::renderer::polygon::<f64>(&pts, &mut opts);
    let fill_opset = roughr::renderer::solid_fill_polygon(&vec![pts.clone()], &mut opts);

    Some((
        ops_to_svg_path_d(&fill_opset),
        ops_to_svg_path_d(&outline_opset),
    ))
}

pub(in crate::svg::parity) fn roughjs_paths_for_circle(
    diameter: f64,
    stroke_width: f32,
    stroke_dasharray: &str,
    hand_drawn: bool,
    randomness: &RoughRandomness,
) -> Option<(String, String)> {
    let (dash0, dash1) = parse_stroke_dash_pair(stroke_dasharray);
    let mut options = roughr::core::OptionsBuilder::default()
        .randomness(randomness.clone())
        .roughness(if hand_drawn { 0.7 } else { 0.0 })
        .fill_style(if hand_drawn {
            roughr::core::FillStyle::Hachure
        } else {
            roughr::core::FillStyle::Solid
        })
        .fill_weight(1.5)
        .hachure_gap(1.5)
        .stroke_width(stroke_width)
        .stroke_line_dash(vec![dash0, dash1])
        .stroke_line_dash_offset(0.0)
        .fill_line_dash(vec![0.0, 0.0])
        .fill_line_dash_offset(0.0)
        .disable_multi_stroke(false)
        .disable_multi_stroke_fill(false)
        .build()
        .ok()?;

    let params = roughr::renderer::generate_ellipse_params(diameter, diameter, &mut options);
    let stroke = roughr::renderer::ellipse_with_params(0.0, 0.0, &mut options, &params);
    let fill = if hand_drawn {
        roughr::renderer::pattern_fill_polygons(vec![stroke.estimated_points], &mut options)
    } else {
        roughr::renderer::ellipse_with_params(0.0, 0.0, &mut options, &params).opset
    };
    Some((ops_to_svg_path_d(&fill), ops_to_svg_path_d(&stroke.opset)))
}
