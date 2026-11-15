// Copyright 2020 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

// Based on https://github.com/fschutt/fastblur

#![allow(clippy::needless_range_loop)]

use super::ImageRefMut;
use rgb::RGBA8;
use std::cmp;

const STEPS: usize = 5;

/// Applies a box blur.
///
/// Input image pixels should have a **premultiplied alpha**.
///
/// A negative or zero `sigma_x`/`sigma_y` will disable the blur along that axis.
///
/// # Allocations
///
/// This method will allocate a copy of the `src` image as a back buffer.
pub fn apply(sigma_x: f64, sigma_y: f64, mut src: ImageRefMut) {
    let boxes_horz = create_box_gauss(sigma_x);
    let boxes_vert = create_box_gauss(sigma_y);
    let mut backbuf =
        match crate::raster::ScratchVec::try_from_slice(src.resources.as_ref(), src.data) {
            Ok(value) => value,
            Err(_) => return,
        };
    let mut backbuf = ImageRefMut::new(src.width, src.height, &mut backbuf, src.resources.clone());

    for (box_size_horz, box_size_vert) in boxes_horz.iter().zip(boxes_vert.iter()) {
        let radius_horz = (box_size_horz - 1.0) / 2.0;
        let radius_vert = (box_size_vert - 1.0) / 2.0;
        if radius_vert >= src.height as f64 {
            box_blur_wide(radius_vert, true, &src, &mut backbuf);
        } else {
            box_blur_vert(radius_vert as usize, &src, &mut backbuf);
        }
        if radius_horz >= src.width as f64 {
            box_blur_wide(radius_horz, false, &backbuf, &mut src);
        } else {
            box_blur_horz(radius_horz as usize, &backbuf, &mut src);
        }
    }
}

#[inline(never)]
fn create_box_gauss(sigma: f64) -> [f64; STEPS] {
    // Keep the original f32 arithmetic for its defined range, including pixel rounding.
    let narrow = sigma as f32;
    let ideal = (12.0 * narrow * narrow / STEPS as f32).sqrt() + 1.0;
    if ideal.floor() as f64 > (i32::MAX - 2) as f64 {
        let n = STEPS as f64;
        let mut lower = (12.0 * sigma * sigma / n).sqrt().floor() + 1.0;
        // At magnitudes without an odd representable integer, the difference of
        // one cell is below f64 precision; all raster samples still use the full
        // transparent-boundary denominator, never a radius clipped to the image.
        if lower % 2.0 == 0.0 {
            lower -= 1.0;
        }
        let upper = lower + 2.0;
        let count = ((12.0 * sigma * sigma - n * lower * lower - 4.0 * n * lower - 3.0)
            / (-4.0 * lower - 4.0))
            .round();
        return std::array::from_fn(|i| if (i as f64) < count { lower } else { upper });
    }
    create_box_gauss_narrow(narrow).map(f64::from)
}

fn create_box_gauss_narrow(sigma: f32) -> [i32; STEPS] {
    if sigma > 0.0 {
        let n_float = STEPS as f32;

        // Ideal averaging filter width
        let w_ideal = (12.0 * sigma * sigma / n_float).sqrt() + 1.0;
        let mut wl = w_ideal.floor() as i32;
        if wl % 2 == 0 {
            wl -= 1;
        }

        let wu = wl + 2;

        let wl_float = wl as f32;
        let m_ideal = (12.0 * sigma * sigma
            - n_float * wl_float * wl_float
            - 4.0 * n_float * wl_float
            - 3.0 * n_float)
            / (-4.0 * wl_float - 4.0);
        let m = m_ideal.round() as usize;

        let mut sizes = [0; STEPS];
        for i in 0..STEPS {
            if i < m {
                sizes[i] = wl;
            } else {
                sizes[i] = wu;
            }
        }

        sizes
    } else {
        [1; STEPS]
    }
}

// A wide box uses sliding sums over actual pixels. The rest of the kernel is
// transparent, so neither indices nor work grow with the astronomical radius.
fn box_blur_wide(radius: f64, vertical: bool, from: &ImageRefMut, to: &mut ImageRefMut) {
    let mut work = super::Work::new(to.resources.as_ref());
    let width = from.width as usize;
    let height = from.height as usize;
    let (length, lines, stride) = if vertical {
        (height, width, width)
    } else {
        (width, height, 1)
    };
    let denominator = 2.0 * radius + 1.0;
    let clipped_radius = radius.min(length as f64) as usize;
    for line in 0..lines {
        let start = if vertical { line } else { line * width };
        let mut sum = [0u64; 4];
        for i in 0..(clipped_radius.saturating_add(1)).min(length) {
            if !work.step() {
                return;
            }
            let p = from.data[start + i * stride];
            for (s, v) in sum.iter_mut().zip([p.r, p.g, p.b, p.a]) {
                *s += v as u64;
            }
        }
        for i in 0..length {
            if !work.step() {
                return;
            }
            let v = sum.map(|v| {
                if denominator <= (i32::MAX - 2) as f64 {
                    round(v as f32 * (1.0 / denominator as f32)) as u8
                } else {
                    (v as f64 / denominator + 0.5).floor() as u8
                }
            });
            to.data[start + i * stride] = RGBA8 {
                r: v[0],
                g: v[1],
                b: v[2],
                a: v[3],
            };
            if i >= clipped_radius {
                let p = from.data[start + (i - clipped_radius) * stride];
                for (s, v) in sum.iter_mut().zip([p.r, p.g, p.b, p.a]) {
                    *s -= v as u64;
                }
            }
            if clipped_radius < length - 1 - i {
                let p = from.data[start + (i + clipped_radius + 1) * stride];
                for (s, v) in sum.iter_mut().zip([p.r, p.g, p.b, p.a]) {
                    *s += v as u64;
                }
            }
        }
    }
}

#[inline]
fn box_blur_vert(blur_radius: usize, backbuf: &ImageRefMut, frontbuf: &mut ImageRefMut) {
    let resources = frontbuf.resources.clone();
    let mut work = super::Work::new(resources.as_ref());
    if blur_radius == 0 {
        for (from, to) in backbuf
            .data
            .chunks(4096)
            .zip(frontbuf.data.chunks_mut(4096))
        {
            if let Some(r) = resources.as_ref() {
                if r.checkpoint(from.len() as u64).is_err() {
                    return;
                }
            }
            to.copy_from_slice(from);
        }
        return;
    }

    let width = backbuf.width as usize;
    let height = backbuf.height as usize;

    let iarr = 1.0 / (blur_radius + blur_radius + 1) as f32;
    let blur_radius_prev = blur_radius as isize - height as isize;
    let blur_radius_next = blur_radius as isize + 1;

    for i in 0..width {
        if !work.step() {
            return;
        }
        let col_start = i; //inclusive
        let col_end = i + width * (height - 1); //inclusive
        let mut ti = i;
        let mut li = ti;
        let mut ri = ti + blur_radius * width;

        let fv = RGBA8::default();
        let lv = RGBA8::default();

        let mut val_r = blur_radius_next * (fv.r as isize);
        let mut val_g = blur_radius_next * (fv.g as isize);
        let mut val_b = blur_radius_next * (fv.b as isize);
        let mut val_a = blur_radius_next * (fv.a as isize);

        // Get the pixel at the specified index, or the first pixel of the column
        // if the index is beyond the top edge of the image
        let get_top = |i| {
            if i < col_start {
                fv
            } else {
                backbuf.data[i]
            }
        };

        // Get the pixel at the specified index, or the last pixel of the column
        // if the index is beyond the bottom edge of the image
        let get_bottom = |i| {
            if i > col_end {
                lv
            } else {
                backbuf.data[i]
            }
        };

        for j in 0..cmp::min(blur_radius, height) {
            if !work.step() {
                return;
            }
            let bb = backbuf.data[ti + j * width];
            val_r += bb.r as isize;
            val_g += bb.g as isize;
            val_b += bb.b as isize;
            val_a += bb.a as isize;
        }
        if blur_radius > height {
            val_r += blur_radius_prev * (lv.r as isize);
            val_g += blur_radius_prev * (lv.g as isize);
            val_b += blur_radius_prev * (lv.b as isize);
            val_a += blur_radius_prev * (lv.a as isize);
        }

        for _ in 0..cmp::min(height, blur_radius + 1) {
            if !work.step() {
                return;
            }
            let bb = get_bottom(ri);
            ri += width;
            val_r += sub(bb.r, fv.r);
            val_g += sub(bb.g, fv.g);
            val_b += sub(bb.b, fv.b);
            val_a += sub(bb.a, fv.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += width;
        }

        if height <= blur_radius {
            // otherwise `(height - blur_radius)` will underflow
            continue;
        }

        for _ in (blur_radius + 1)..(height - blur_radius) {
            if !work.step() {
                return;
            }
            let bb1 = backbuf.data[ri];
            ri += width;
            let bb2 = backbuf.data[li];
            li += width;

            val_r += sub(bb1.r, bb2.r);
            val_g += sub(bb1.g, bb2.g);
            val_b += sub(bb1.b, bb2.b);
            val_a += sub(bb1.a, bb2.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += width;
        }

        for _ in 0..cmp::min(height - blur_radius - 1, blur_radius) {
            if !work.step() {
                return;
            }
            let bb = get_top(li);
            li += width;

            val_r += sub(lv.r, bb.r);
            val_g += sub(lv.g, bb.g);
            val_b += sub(lv.b, bb.b);
            val_a += sub(lv.a, bb.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += width;
        }
    }
}

#[inline]
fn box_blur_horz(blur_radius: usize, backbuf: &ImageRefMut, frontbuf: &mut ImageRefMut) {
    let resources = frontbuf.resources.clone();
    let mut work = super::Work::new(resources.as_ref());
    if blur_radius == 0 {
        for (from, to) in backbuf
            .data
            .chunks(4096)
            .zip(frontbuf.data.chunks_mut(4096))
        {
            if let Some(r) = resources.as_ref() {
                if r.checkpoint(from.len() as u64).is_err() {
                    return;
                }
            }
            to.copy_from_slice(from);
        }
        return;
    }

    let width = backbuf.width as usize;
    let height = backbuf.height as usize;

    let iarr = 1.0 / (blur_radius + blur_radius + 1) as f32;
    let blur_radius_prev = blur_radius as isize - width as isize;
    let blur_radius_next = blur_radius as isize + 1;

    for i in 0..height {
        if !work.step() {
            return;
        }
        let row_start = i * width; // inclusive
        let row_end = (i + 1) * width - 1; // inclusive
        let mut ti = i * width; // VERTICAL: $i;
        let mut li = ti;
        let mut ri = ti + blur_radius;

        let fv = RGBA8::default();
        let lv = RGBA8::default();

        let mut val_r = blur_radius_next * (fv.r as isize);
        let mut val_g = blur_radius_next * (fv.g as isize);
        let mut val_b = blur_radius_next * (fv.b as isize);
        let mut val_a = blur_radius_next * (fv.a as isize);

        // Get the pixel at the specified index, or the first pixel of the row
        // if the index is beyond the left edge of the image
        let get_left = |i| {
            if i < row_start {
                fv
            } else {
                backbuf.data[i]
            }
        };

        // Get the pixel at the specified index, or the last pixel of the row
        // if the index is beyond the right edge of the image
        let get_right = |i| {
            if i > row_end {
                lv
            } else {
                backbuf.data[i]
            }
        };

        for j in 0..cmp::min(blur_radius, width) {
            if !work.step() {
                return;
            }
            let bb = backbuf.data[ti + j]; // VERTICAL: ti + j * width
            val_r += bb.r as isize;
            val_g += bb.g as isize;
            val_b += bb.b as isize;
            val_a += bb.a as isize;
        }
        if blur_radius > width {
            val_r += blur_radius_prev * (lv.r as isize);
            val_g += blur_radius_prev * (lv.g as isize);
            val_b += blur_radius_prev * (lv.b as isize);
            val_a += blur_radius_prev * (lv.a as isize);
        }

        // Process the left side where we need pixels from beyond the left edge
        for _ in 0..cmp::min(width, blur_radius + 1) {
            if !work.step() {
                return;
            }
            let bb = get_right(ri);
            ri += 1;
            val_r += sub(bb.r, fv.r);
            val_g += sub(bb.g, fv.g);
            val_b += sub(bb.b, fv.b);
            val_a += sub(bb.a, fv.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += 1; // VERTICAL : ti += width, same with the other areas
        }

        if width <= blur_radius {
            // otherwise `(width - blur_radius)` will underflow
            continue;
        }

        // Process the middle where we know we won't bump into borders
        // without the extra indirection of get_left/get_right. This is faster.
        for _ in (blur_radius + 1)..(width - blur_radius) {
            if !work.step() {
                return;
            }
            let bb1 = backbuf.data[ri];
            ri += 1;
            let bb2 = backbuf.data[li];
            li += 1;

            val_r += sub(bb1.r, bb2.r);
            val_g += sub(bb1.g, bb2.g);
            val_b += sub(bb1.b, bb2.b);
            val_a += sub(bb1.a, bb2.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += 1;
        }

        // Process the right side where we need pixels from beyond the right edge
        for _ in 0..cmp::min(width - blur_radius - 1, blur_radius) {
            if !work.step() {
                return;
            }
            let bb = get_left(li);
            li += 1;

            val_r += sub(lv.r, bb.r);
            val_g += sub(lv.g, bb.g);
            val_b += sub(lv.b, bb.b);
            val_a += sub(lv.a, bb.a);

            frontbuf.data[ti] = RGBA8 {
                r: round(val_r as f32 * iarr) as u8,
                g: round(val_g as f32 * iarr) as u8,
                b: round(val_b as f32 * iarr) as u8,
                a: round(val_a as f32 * iarr) as u8,
            };
            ti += 1;
        }
    }
}

/// Fast rounding for x <= 2^23.
/// This is orders of magnitude faster than built-in rounding intrinsic.
///
/// Source: https://stackoverflow.com/a/42386149/585725
#[inline]
fn round(mut x: f32) -> f32 {
    x += 12582912.0;
    x -= 12582912.0;
    x
}

#[inline]
fn sub(c1: u8, c2: u8) -> isize {
    c1 as isize - c2 as isize
}
