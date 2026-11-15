// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Independent borrowed-A8 geometry-coverage tests for the checked painter.
//!
//! No candidate painter or raster helper supplies the expected pixels. The
//! integer oracle below evaluates
//! Porter-Duff equations over exact rational straight source / premultiplied
//! destination values. Automatic and forced-HQ pipelines are checked separately;
//! their interior quantization may differ by one byte. Exact cases are separate.
//!
//! Checkpoint tests establish bounded positive charges, pre-write cancellation,
//! and an untouched unfunded suffix. An immutable MaskRef black box cannot expose
//! source read timing safely; these tests do not claim a dynamic proof of no read
//! before payment. That property also requires inspection of the production loop.

use resvg::tiny_skia::{
    BlendMode, Color, CoveragePaintError, FilterQuality, GradientStop, LinearGradient, MaskRef,
    Paint, Pattern, PixmapMut, PixmapRef, Point, Shader, SpreadMode, Transform,
};

const A8: [u8; 6] = [0, 1, 127, 128, 254, 255];

fn round_ratio(numerator: u64, denominator: u64) -> u8 {
    let result = (numerator + denominator / 2) / denominator;
    u8::try_from(result).expect("the independently calculated channel is a byte")
}

/// Source is straight RGBA8; destination and the result are premultiplied RGBA8.
/// The single final rounding is mathematical, not a copy of a pipeline stage.
fn rational_pixel(mode: BlendMode, source: [u8; 4], dest: [u8; 4], mask: u8) -> [u8; 4] {
    let a = u64::from(source[3]);
    let da = u64::from(dest[3]);
    let k = u64::from(mask);
    let straight = [source[0], source[1], source[2], 255];
    std::array::from_fn(|channel| {
        let c = u64::from(straight[channel]);
        let d = u64::from(dest[channel]);
        match mode {
            BlendMode::Clear => round_ratio(d * (255 - k), 255),
            BlendMode::Source => round_ratio(c * a * k + d * (255 - k) * 255, 255 * 255),
            BlendMode::SourceOver => round_ratio(c * a * k + d * (255 * 255 - a * k), 255 * 255),
            BlendMode::Xor => round_ratio(
                c * a * (255 - da) * k + d * (255 * 255 - a * k) * 255,
                255 * 255 * 255,
            ),
            _ => panic!("this finite independent oracle covers only four selected modes"),
        }
    })
}

fn solid_paint(source: [u8; 4], blend_mode: BlendMode, force_hq_pipeline: bool) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(Color::from_rgba8(
            source[0], source[1], source[2], source[3],
        )),
        blend_mode,
        force_hq_pipeline,
        ..Paint::default()
    }
}

fn repeated_pixel(pixel: [u8; 4], count: usize) -> Vec<u8> {
    (0..count).flat_map(|_| pixel).collect()
}

fn assert_pixel_close(actual: &[u8], expected: [u8; 4], tolerance: u8) {
    assert_eq!(actual.len(), 4);
    for channel in 0..4 {
        assert!(
            actual[channel].abs_diff(expected[channel]) <= tolerance,
            "channel {channel}: actual {actual:?}, exact rational rounded {expected:?}, tolerance {tolerance}"
        );
    }
}

fn paint_ok(
    destination: &mut [u8],
    width: u32,
    height: u32,
    coverage: &[u8],
    paint: &Paint<'_>,
) -> Vec<u64> {
    let mut calls = Vec::new();
    let result = PixmapMut::from_bytes(destination, width, height)
        .unwrap()
        .paint_coverage_checked(
            MaskRef::from_bytes(coverage, width, height).unwrap(),
            paint,
            |pixels| {
                calls.push(pixels);
                Ok::<(), ()>(())
            },
        );
    assert!(result.is_ok(), "valid coverage paint failed: {result:?}");
    calls
}

fn assert_pixel_charges(calls: &[u64], pixels: usize) {
    // Entry / exit zero checkpoints are permitted, including a final zero after
    // all pixels were processed. Zero never stands for another 4096 pixels.
    assert!(calls.iter().all(|&n| n <= 4096), "charges: {calls:?}");
    assert_eq!(calls.iter().sum::<u64>(), pixels as u64);
}

#[test]
fn coverage_blends_match_independent_rationals_at_a8_boundaries() {
    let source = [207, 83, 29, 173];
    for hq in [false, true] {
        for mode in [
            BlendMode::SourceOver,
            BlendMode::Clear,
            BlendMode::Source,
            BlendMode::Xor,
        ] {
            for dest in [[0, 0, 0, 0], [21, 71, 92, 137], [19, 113, 239, 255]] {
                let mut output = repeated_pixel(dest, A8.len());
                let calls = paint_ok(&mut output, 6, 1, &A8, &solid_paint(source, mode, hq));
                assert_pixel_charges(&calls, A8.len());
                for (index, pixel) in output.chunks_exact(4).enumerate() {
                    let expected = rational_pixel(mode, source, dest, A8[index]);
                    assert_pixel_close(pixel, expected, 1);
                    if A8[index] == 0 || (mode == BlendMode::Clear && A8[index] == 255) {
                        assert_eq!(
                            pixel, expected,
                            "zero coverage and Clear endpoints are exact"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn coverage_opaque_source_over_and_clear_primary_channels_are_exact() {
    for hq in [false, true] {
        for (mode, dest) in [
            (BlendMode::SourceOver, [0, 0, 0, 0]),
            (BlendMode::SourceOver, [0, 0, 0, 255]),
            (BlendMode::Clear, [255, 0, 0, 255]),
        ] {
            let source = [255, 0, 0, 255];
            let mut output = repeated_pixel(dest, A8.len());
            let calls = paint_ok(&mut output, 6, 1, &A8, &solid_paint(source, mode, hq));
            assert_pixel_charges(&calls, A8.len());
            for (index, pixel) in output.chunks_exact(4).enumerate() {
                assert_eq!(pixel, rational_pixel(mode, source, dest, A8[index]));
            }
        }
    }
}

#[test]
fn coverage_multiple_odd_rows_over_4096_pixels_preserve_caller_guards() {
    const WIDTH: u32 = 2051;
    const HEIGHT: u32 = 3;
    const GUARD: usize = 19;
    let count = WIDTH as usize * HEIGHT as usize;
    let source = [203, 77, 31, 181];
    let dest = [21, 61, 93, 141];
    let mut mask_storage = vec![0xB7; GUARD + count + GUARD];
    for (index, byte) in mask_storage[GUARD..GUARD + count].iter_mut().enumerate() {
        *byte = A8[index % A8.len()];
    }
    let original_mask = mask_storage.clone();
    let mut storage = vec![0xA5; GUARD + count * 4 + GUARD];
    storage[GUARD..GUARD + count * 4].copy_from_slice(&repeated_pixel(dest, count));
    let calls = paint_ok(
        &mut storage[GUARD..GUARD + count * 4],
        WIDTH,
        HEIGHT,
        &mask_storage[GUARD..GUARD + count],
        &solid_paint(source, BlendMode::SourceOver, false),
    );
    assert_pixel_charges(&calls, count);
    assert!(calls.iter().filter(|&&n| n != 0).count() >= 2);
    assert!(storage[..GUARD].iter().all(|&b| b == 0xA5));
    assert!(storage[GUARD + count * 4..].iter().all(|&b| b == 0xA5));
    assert_eq!(mask_storage, original_mask);
    for (index, pixel) in storage[GUARD..GUARD + count * 4]
        .chunks_exact(4)
        .enumerate()
    {
        assert_pixel_close(
            pixel,
            rational_pixel(BlendMode::SourceOver, source, dest, A8[index % A8.len()]),
            1,
        );
    }
}

#[derive(Debug, PartialEq)]
struct Stop(u32);

#[test]
fn coverage_first_positive_checkpoint_failure_leaves_all_destination_bytes() {
    let mask = [255; 15];
    let initial = repeated_pixel([11, 22, 33, 99], mask.len());
    let mut output = initial.clone();
    let mut rejected = false;
    let result = PixmapMut::from_bytes(&mut output, 5, 3)
        .unwrap()
        .paint_coverage_checked(
            MaskRef::from_bytes(&mask, 5, 3).unwrap(),
            &solid_paint([255, 0, 0, 255], BlendMode::SourceOver, false),
            |pixels| {
                assert!(pixels <= 4096);
                if pixels == 0 {
                    Ok(())
                } else {
                    rejected = true;
                    Err(Stop(17))
                }
            },
        );
    assert!(rejected);
    assert!(matches!(
        result,
        Err(CoveragePaintError::Checkpoint(Stop(17)))
    ));
    assert_eq!(output, initial);
}

#[test]
fn coverage_later_checkpoint_failure_never_writes_the_unfunded_suffix() {
    let count = 2051 * 3;
    let mask = vec![255; count];
    let dest = [11, 22, 33, 99];
    let initial = repeated_pixel(dest, count);
    let mut output = initial.clone();
    let mut approved = 0_u64;
    let mut rejected = false;
    let result = PixmapMut::from_bytes(&mut output, 2051, 3)
        .unwrap()
        .paint_coverage_checked(
            MaskRef::from_bytes(&mask, 2051, 3).unwrap(),
            &solid_paint([255, 0, 0, 255], BlendMode::SourceOver, false),
            |pixels| {
                assert!(pixels <= 4096);
                if pixels == 0 {
                    return Ok(());
                }
                if approved != 0 {
                    rejected = true;
                    return Err(Stop(29));
                }
                approved += pixels;
                Ok(())
            },
        );
    assert!(rejected && approved > 0 && approved <= 4096);
    assert!(matches!(
        result,
        Err(CoveragePaintError::Checkpoint(Stop(29)))
    ));
    assert_eq!(
        &output[approved as usize * 4..],
        &initial[approved as usize * 4..]
    );
    let mut changed = 0;
    for pixel in output[..approved as usize * 4].chunks_exact(4) {
        assert!(pixel == dest || pixel == [255, 0, 0, 255]);
        changed += usize::from(pixel != dest);
    }
    assert!(
        changed != 0,
        "the later failure must exercise partial output"
    );
}

#[test]
fn coverage_dimension_mismatch_with_equal_pixel_count_is_typed_and_does_not_write() {
    let mask = [255; 6];
    let mut output = repeated_pixel([21, 61, 93, 141], 6);
    let initial = output.clone();
    let result = PixmapMut::from_bytes(&mut output, 3, 2)
        .unwrap()
        .paint_coverage_checked(
            MaskRef::from_bytes(&mask, 2, 3).unwrap(),
            &solid_paint([255, 0, 0, 255], BlendMode::Clear, false),
            |_| Ok::<(), ()>(()),
        );
    assert!(matches!(result, Err(CoveragePaintError::SizeMismatch)));
    assert_eq!(output, initial);
}

#[test]
fn coverage_false_antialias_is_rejected_even_for_clear_and_destination() {
    for mode in [BlendMode::Clear, BlendMode::Destination] {
        let mut paint = solid_paint([255, 0, 0, 255], mode, false);
        paint.anti_alias = false;
        let mask = [255; 6];
        let mut output = repeated_pixel([21, 61, 93, 141], 6);
        let initial = output.clone();
        let result = PixmapMut::from_bytes(&mut output, 3, 2)
            .unwrap()
            .paint_coverage_checked(MaskRef::from_bytes(&mask, 3, 2).unwrap(), &paint, |_| {
                Ok::<(), ()>(())
            });
        assert!(matches!(result, Err(CoveragePaintError::InvalidPaint)));
        assert_eq!(output, initial);
    }
}

fn rank_one_shader_transform() -> Transform {
    // Exact rank-one matrix with nonzero skew: determinant 1*1 - 1*1 = 0.
    // The all-scale fast path of Transform::invert returns Some even for a
    // zero scale, so it cannot establish the shader-stage failure premise.
    let transform = Transform::from_row(1.0, 1.0, 1.0, 1.0, 0.0, 0.0);
    assert!(transform.invert().is_none());
    transform
}

fn singular_gradient(transform: Transform) -> Shader<'static> {
    let mut shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(4.0, 0.0),
        vec![
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    assert!(matches!(&shader, Shader::LinearGradient(_)));
    // Establish a real gradient before applying the test matrix, so constructor
    // degeneration into a legal SolidColor cannot hide this error case.
    shader.transform(transform);
    assert!(matches!(&shader, Shader::LinearGradient(_)));
    shader
}

#[test]
fn coverage_legal_noops_are_distinct_from_gradient_and_pattern_shader_failures() {
    let mask = A8;
    let dest = [21, 61, 93, 141];
    let initial = repeated_pixel(dest, mask.len());
    let zero_scale = Transform::from_scale(0.0, 1.0);
    // Retain the original failing input: legacy inversion returns Some with
    // nonfinite coefficients. The checked painter must not silently paint it.
    assert!(!zero_scale.invert().unwrap().is_finite());
    let rank_one = rank_one_shader_transform();
    let noops = [
        Paint {
            shader: singular_gradient(zero_scale),
            blend_mode: BlendMode::Destination,
            ..Paint::default()
        },
        solid_paint([255, 255, 255, 255], BlendMode::DestinationIn, false),
    ];
    for paint in noops {
        let mut output = initial.clone();
        // A legitimate no-op may perform only an entry checkpoint. Do not infer
        // full pixel processing or impose a mirrored checkpoint count here.
        paint_ok(&mut output, 6, 1, &mask, &paint);
        assert_eq!(output, initial);
    }

    let texels = [255, 0, 0, 255, 0, 0, 255, 255];
    let zero_scale_pattern = Pattern::new(
        PixmapRef::from_bytes(&texels, 2, 1).unwrap(),
        SpreadMode::Repeat,
        FilterQuality::Nearest,
        1.0,
        zero_scale,
    );
    assert!(matches!(&zero_scale_pattern, Shader::Pattern(_)));
    let rank_one_pattern = Pattern::new(
        PixmapRef::from_bytes(&texels, 2, 1).unwrap(),
        SpreadMode::Repeat,
        FilterQuality::Nearest,
        1.0,
        rank_one,
    );
    assert!(matches!(&rank_one_pattern, Shader::Pattern(_)));

    // Both the original matrix and its inverse are finite, but the gradient's
    // independently known [0, 0.001] -> [0, 1] unit map multiplies x by ~1000.
    // Its composition with inverse x-scale ~1e37 exceeds f32::MAX. This catches
    // a validator that checks only the inverse and omits the actual stage map.
    let short_end = 0.001_f32;
    let overflow_transform = Transform::from_scale(1.0e-37, 1.0);
    assert!(overflow_transform.is_finite());
    let inverse = overflow_transform.invert().unwrap();
    assert!(inverse.is_finite());
    let unit_scale = 1.0 / short_end;
    assert!(f64::from(inverse.sx) * f64::from(unit_scale) > f64::from(f32::MAX));
    assert!(
        !inverse
            .post_concat(Transform::from_scale(unit_scale, unit_scale))
            .is_finite()
    );
    let overflow_gradient = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(short_end, 0.0),
        vec![
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ],
        SpreadMode::Pad,
        overflow_transform,
    )
    .expect("the nondegenerate gradient and finite inverse are valid constructor inputs");
    assert!(matches!(&overflow_gradient, Shader::LinearGradient(_)));
    let failed_shaders = [
        ("zero-scale linear gradient", singular_gradient(zero_scale)),
        ("rank-one linear gradient", singular_gradient(rank_one)),
        ("zero-scale pattern", zero_scale_pattern),
        ("rank-one pattern", rank_one_pattern),
        (
            "finite inverse with overflowing gradient unit map",
            overflow_gradient,
        ),
    ];
    for (name, shader) in failed_shaders {
        let mut output = initial.clone();
        let paint = Paint {
            shader,
            blend_mode: BlendMode::SourceOver,
            ..Paint::default()
        };
        let result = PixmapMut::from_bytes(&mut output, 6, 1)
            .unwrap()
            .paint_coverage_checked(MaskRef::from_bytes(&mask, 6, 1).unwrap(), &paint, |_| {
                Ok::<(), ()>(())
            });
        assert!(
            matches!(result, Err(CoveragePaintError::Shader)),
            "{name}: expected typed Shader rejection, got {result:?}"
        );
        assert_eq!(output, initial);
    }

    // Clear has no source-color dependency. Keep the original singular shaders
    // and require the mathematical geometry operation at 0, 128, and 255 (plus
    // the adjacent A8 boundary values), in both selected precision settings.
    for hq in [false, true] {
        let unrelated_shaders = [
            singular_gradient(zero_scale),
            Pattern::new(
                PixmapRef::from_bytes(&texels, 2, 1).unwrap(),
                SpreadMode::Repeat,
                FilterQuality::Nearest,
                1.0,
                zero_scale,
            ),
        ];
        for shader in unrelated_shaders {
            let paint = Paint {
                shader,
                blend_mode: BlendMode::Clear,
                force_hq_pipeline: hq,
                ..Paint::default()
            };
            let mut output = initial.clone();
            let calls = paint_ok(&mut output, 6, 1, &mask, &paint);
            assert_pixel_charges(&calls, mask.len());
            for (index, pixel) in output.chunks_exact(4).enumerate() {
                let expected = rational_pixel(BlendMode::Clear, [0; 4], dest, mask[index]);
                assert_pixel_close(pixel, expected, 1);
                if mask[index] == 0 || mask[index] == 255 {
                    assert_eq!(
                        pixel, expected,
                        "Clear endpoints remain exact with an unrelated shader"
                    );
                }
            }
        }
    }
}

#[test]
fn coverage_device_gradient_uses_one_transform_and_one_opacity_application() {
    // Local [0,4] -> device [1,9]. For center x+1/2, the independent
    // grayscale parameter is clamp((2*x-1)/16, 0, 1), then opacity is 1/2.
    let mut shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(4.0, 0.0),
        vec![
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    shader.transform(Transform::from_row(2.0, 0.0, 0.0, 1.0, 1.0, 0.0));
    shader.apply_opacity(0.5);
    let mask: Vec<_> = (0..33).map(|index| A8[index % A8.len()]).collect();
    let mut paint = Paint {
        shader,
        ..Paint::default()
    };
    for hq in [false, true] {
        // Borrow the same already device-space shader; the painter receives no M.
        paint.force_hq_pipeline = hq;
        let mut output = vec![0; mask.len() * 4];
        let calls = paint_ok(&mut output, 11, 3, &mask, &paint);
        assert_pixel_charges(&calls, mask.len());
        for (index, pixel) in output.chunks_exact(4).enumerate() {
            let x = (index % 11) as i64;
            let parameter = (2 * x - 1).clamp(0, 16) as u64;
            let k = u64::from(mask[index]);
            let gray = round_ratio(parameter * k, 32);
            assert_pixel_close(pixel, [gray, gray, gray, round_ratio(k, 2)], 1);
        }
    }
}

#[test]
fn coverage_device_pattern_uses_geometry_interpolation_and_one_opacity_application() {
    let texels = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let source_colors = [[255_u64, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]];
    let dest = [21, 61, 93, 141];
    let mask: Vec<_> = (0..30).map(|index| A8[index % A8.len()]).collect();
    for hq in [false, true] {
        let mut shader = Pattern::new(
            PixmapRef::from_bytes(&texels, 2, 2).unwrap(),
            SpreadMode::Repeat,
            FilterQuality::Nearest,
            0.5,
            Transform::identity(),
        );
        shader.transform(Transform::from_row(2.0, 0.0, 0.0, 2.0, 1.0, 1.0));
        let paint = Paint {
            shader,
            blend_mode: BlendMode::Source,
            force_hq_pipeline: hq,
            ..Paint::default()
        };
        let mut output = repeated_pixel(dest, mask.len());
        let calls = paint_ok(&mut output, 6, 5, &mask, &paint);
        assert_pixel_charges(&calls, mask.len());
        for (index, pixel) in output.chunks_exact(4).enumerate() {
            // Nearest repeat at inverse-M(center), using integer Euclidean floor
            // to keep the negative translated first row/column unambiguous.
            let x = (index % 6) as i64;
            let y = (index / 6) as i64;
            let sx = (2 * x - 1).div_euclid(4).rem_euclid(2) as usize;
            let sy = (2 * y - 1).div_euclid(4).rem_euclid(2) as usize;
            let rgb = source_colors[sy * 2 + sx];
            let k = u64::from(mask[index]);
            let expected = std::array::from_fn(|channel| {
                let source = if channel == 3 { 255 } else { rgb[channel] };
                round_ratio(source * k + 2 * u64::from(dest[channel]) * (255 - k), 510)
            });
            assert_pixel_close(pixel, expected, 1);
        }
    }
}

#[test]
fn coverage_color_discontinuity_explicitly_differs_from_full_scene_subsampling() {
    // One target cell has geometry on its left half. The shader is red on that
    // half and blue on the right half; its center sample is blue. The selected
    // E[a]*c(center) model therefore paints blue. Full-scene E[a*c] would paint
    // red. This is a model distinction, not an equivalence or quality assertion.
    let texels = [255, 0, 0, 255, 0, 0, 255, 255];
    let mask = [128];
    let shader = Pattern::new(
        PixmapRef::from_bytes(&texels, 2, 1).unwrap(),
        SpreadMode::Pad,
        FilterQuality::Nearest,
        1.0,
        Transform::from_scale(0.5, 1.0),
    );
    let paint = Paint {
        shader,
        ..Paint::default()
    };
    let mut output = [0; 4];
    let calls = paint_ok(&mut output, 1, 1, &mask, &paint);
    assert_pixel_charges(&calls, 1);
    assert_eq!(output, [0, 0, 128, 128]);
    let full_scene_subsampled = [128, 0, 0, 128];
    assert_ne!(output, full_scene_subsampled);
}
