// Copyright 2020 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

#![allow(clippy::needless_range_loop)]

use super::{f32_bound, ImageRefMut};
use usvg::ApproxZeroUlps;

const RAND_M: i32 = 2147483647; // 2**31 - 1
const RAND_A: i32 = 16807; // 7**5; primitive root of m
const RAND_Q: i32 = 127773; // m / a
const RAND_R: i32 = 2836; // m % a
const B_SIZE: usize = 0x100;
const B_SIZE_32: i32 = 0x100;
const B_LEN: usize = B_SIZE + B_SIZE + 2;
const BM: i32 = 0xff;
const PERLIN_N: i32 = 0x1000;

#[derive(Clone, Copy)]
struct StitchInfo {
    // Only the low lattice bits of an integer period affect the gradient lookup.
    width: i32,
    height: i32,
    // Keep comparison thresholds in f64; growing octave periods must not overflow i32.
    wrap_x: f64,
    wrap_y: f64,
}

/// Applies a turbulence filter.
///
/// `dest` image pixels will have an **unpremultiplied alpha**.
///
/// - `offset_x` and `offset_y` indicate filter region offset.
/// - `sx` and `sy` indicate canvas scale.
pub fn apply(
    offset_x: f64,
    offset_y: f64,
    sx: f64,
    sy: f64,
    base_frequency_x: f64,
    base_frequency_y: f64,
    num_octaves: u32,
    seed: i32,
    stitch_tiles: bool,
    fractal_noise: bool,
    dest: ImageRefMut,
) {
    let resources = dest.resources.clone();
    let mut work = super::Work::new(resources.as_ref());
    let (lattice_selector, gradient) = match init(seed, resources.as_ref()) {
        Ok(value) => value,
        Err(_) => return,
    };
    let width = dest.width;
    let height = dest.height;
    let mut x = 0;
    let mut y = 0;
    for pixel in dest.data.iter_mut() {
        if !work.step() {
            return;
        }
        let mut turb = |channel| {
            let (tx, ty) = ((x as f64 + offset_x) / sx, (y as f64 + offset_y) / sy);
            let n = turbulence(
                channel,
                tx,
                ty,
                x as f64,
                y as f64,
                width as f64,
                height as f64,
                base_frequency_x,
                base_frequency_y,
                num_octaves,
                fractal_noise,
                stitch_tiles,
                &lattice_selector,
                &gradient,
                &mut work,
            );

            let n = if fractal_noise {
                (n * 255.0 + 255.0) / 2.0
            } else {
                n * 255.0
            };

            (f32_bound(0.0, n as f32, 255.0) + 0.5) as u8
        };

        pixel.r = turb(0);
        pixel.g = turb(1);
        pixel.b = turb(2);
        pixel.a = turb(3);

        x += 1;
        if x == dest.width {
            x = 0;
            y += 1;
        }
    }
}

fn init(
    mut seed: i32,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Result<
    (
        crate::raster::ScratchVec<usize>,
        crate::raster::ScratchVec<[[f64; 2]; B_LEN]>,
    ),
    markdown_svg_budget::Error,
> {
    let mut work = super::Work::new(resources);
    let mut lattice_selector = crate::raster::ScratchVec::try_filled(resources, B_LEN, 0usize)?;
    let mut gradient = crate::raster::ScratchVec::try_filled(resources, 4, [[0.0; 2]; B_LEN])?;

    if seed <= 0 {
        seed = (-(seed as i64) % i64::from(RAND_M - 1) + 1) as i32;
    }

    if seed > RAND_M - 1 {
        seed = RAND_M - 1;
    }

    for k in 0..4 {
        if !work.step() {
            return Err(resources.unwrap().failure().unwrap());
        }
        for i in 0..B_SIZE {
            if !work.step() {
                return Err(resources.unwrap().failure().unwrap());
            }
            lattice_selector[i] = i;
            for j in 0..2 {
                if !work.step() {
                    return Err(resources.unwrap().failure().unwrap());
                }
                seed = random(seed);
                gradient[k][i][j] =
                    ((seed % (B_SIZE_32 + B_SIZE_32)) - B_SIZE_32) as f64 / B_SIZE_32 as f64;
            }

            let s = (gradient[k][i][0] * gradient[k][i][0] + gradient[k][i][1] * gradient[k][i][1])
                .sqrt();

            gradient[k][i][0] /= s;
            gradient[k][i][1] /= s;
        }
    }

    for i in (1..B_SIZE).rev() {
        if !work.step() {
            return Err(resources.unwrap().failure().unwrap());
        }
        let k = lattice_selector[i];
        seed = random(seed);
        let j = (seed % B_SIZE_32) as usize;
        lattice_selector[i] = lattice_selector[j];
        lattice_selector[j] = k;
    }

    for i in 0..B_SIZE + 2 {
        if !work.step() {
            return Err(resources.unwrap().failure().unwrap());
        }
        lattice_selector[B_SIZE + i] = lattice_selector[i];
        for g in gradient.iter_mut().take(4) {
            if !work.step() {
                return Err(resources.unwrap().failure().unwrap());
            }
            for j in 0..2 {
                if !work.step() {
                    return Err(resources.unwrap().failure().unwrap());
                }
                g[B_SIZE + i][j] = g[i][j];
            }
        }
    }

    Ok((lattice_selector, gradient))
}

fn turbulence(
    color_channel: usize,
    mut x: f64,
    mut y: f64,
    tile_x: f64,
    tile_y: f64,
    tile_width: f64,
    tile_height: f64,
    mut base_freq_x: f64,
    mut base_freq_y: f64,
    num_octaves: u32,
    fractal_sum: bool,
    do_stitching: bool,
    lattice_selector: &[usize],
    gradient: &[[[f64; 2]; B_LEN]],
    work: &mut super::Work<'_>,
) -> f64 {
    // Adjust the base frequencies if necessary for stitching.
    let mut stitch = if do_stitching {
        // When stitching tiled turbulence, the frequencies must be adjusted
        // so that the tile borders will be continuous.
        if !base_freq_x.approx_zero_ulps(4) {
            let lo_freq = (tile_width * base_freq_x).floor() / tile_width;
            let hi_freq = (tile_width * base_freq_x).ceil() / tile_width;
            if base_freq_x / lo_freq < hi_freq / base_freq_x {
                base_freq_x = lo_freq;
            } else {
                base_freq_x = hi_freq;
            }
        }

        if !base_freq_y.approx_zero_ulps(4) {
            let lo_freq = (tile_height * base_freq_y).floor() / tile_height;
            let hi_freq = (tile_height * base_freq_y).ceil() / tile_height;
            if base_freq_y / lo_freq < hi_freq / base_freq_y {
                base_freq_y = lo_freq;
            } else {
                base_freq_y = hi_freq;
            }
        }

        // Set up initial stitch values.
        let width = (tile_width * base_freq_x + 0.5).trunc();
        let height = (tile_height * base_freq_y + 0.5).trunc();
        let wrap_x = (tile_x * base_freq_x + PERLIN_N as f64 + width).trunc();
        let wrap_y = (tile_y * base_freq_y + PERLIN_N as f64 + height).trunc();
        let width = width.rem_euclid(B_SIZE as f64) as i32;
        let height = height.rem_euclid(B_SIZE as f64) as i32;
        Some(StitchInfo {
            width,
            height,
            wrap_x,
            wrap_y,
        })
    } else {
        None
    };

    let mut sum = 0.0;
    x *= base_freq_x;
    y *= base_freq_y;
    let mut weight = 1.0;
    for _ in 0..num_octaves {
        if !work.step() {
            return 0.0;
        }
        if fractal_sum {
            sum += noise2(color_channel, x, y, lattice_selector, gradient, stitch) * weight;
        } else {
            sum += noise2(color_channel, x, y, lattice_selector, gradient, stitch).abs() * weight;
        }
        // Beyond this magnitude f64 has no fractional lattice component. The
        // noise table is periodic in 256 cells. Once a stitch period is a
        // multiple of 256, reducing that axis by whole periods is exact for
        // lookup and avoids producing infinity in later octaves.
        if x.abs() > 1.0e100 && stitch.is_none_or(|s| s.width == 0) {
            x = x.rem_euclid(B_SIZE as f64);
        }
        if y.abs() > 1.0e100 && stitch.is_none_or(|s| s.height == 0) {
            y = y.rem_euclid(B_SIZE as f64);
        }
        x *= 2.0;
        y *= 2.0;
        weight *= 0.5;

        if let Some(ref mut stitch) = stitch {
            // Update stitch values. Subtracting PerlinN before the multiplication and
            // adding it afterward simplifies to subtracting it once.
            stitch.width = (stitch.width * 2) & BM;
            if stitch.width != 0 {
                stitch.wrap_x = 2.0 * stitch.wrap_x - PERLIN_N as f64;
            }
            stitch.height = (stitch.height * 2) & BM;
            if stitch.height != 0 {
                stitch.wrap_y = 2.0 * stitch.wrap_y - PERLIN_N as f64;
            }
        }
    }

    sum
}

fn noise2(
    color_channel: usize,
    x: f64,
    y: f64,
    lattice_selector: &[usize],
    gradient: &[[[f64; 2]; B_LEN]],
    stitch_info: Option<StitchInfo>,
) -> f64 {
    let tx = x + PERLIN_N as f64;
    let ty = y + PERLIN_N as f64;
    let ix = tx.trunc();
    let iy = ty.trunc();
    let rx0 = tx - ix;
    let rx1 = rx0 - 1.0;
    let ry0 = ty - iy;
    let ry1 = ry0 - 1.0;
    let mut bx0 = ix.rem_euclid(B_SIZE as f64) as i32;
    let mut bx1 = (bx0 + 1) & BM;
    let mut by0 = iy.rem_euclid(B_SIZE as f64) as i32;
    let mut by1 = (by0 + 1) & BM;

    // Stitch comparisons use the original integer lattice coordinate; period
    // subtraction may be reduced modulo256 before indexing the periodic table.
    if let Some(info) = stitch_info {
        if ix >= info.wrap_x {
            bx0 -= info.width;
        }
        if ix + 1.0 >= info.wrap_x {
            bx1 -= info.width;
        }
        if iy >= info.wrap_y {
            by0 -= info.height;
        }
        if iy + 1.0 >= info.wrap_y {
            by1 -= info.height;
        }
    }

    bx0 &= BM;
    bx1 &= BM;
    by0 &= BM;
    by1 &= BM;
    let i = lattice_selector[bx0 as usize];
    let j = lattice_selector[bx1 as usize];
    let b00 = lattice_selector[i + by0 as usize];
    let b10 = lattice_selector[j + by0 as usize];
    let b01 = lattice_selector[i + by1 as usize];
    let b11 = lattice_selector[j + by1 as usize];
    let sx = s_curve(rx0);
    let sy = s_curve(ry0);
    let q = &gradient[color_channel][b00];
    let u = rx0 * q[0] + ry0 * q[1];
    let q = &gradient[color_channel][b10];
    let v = rx1 * q[0] + ry0 * q[1];
    let a = lerp(sx, u, v);
    let q = &gradient[color_channel][b01];
    let u = rx0 * q[0] + ry1 * q[1];
    let q = &gradient[color_channel][b11];
    let v = rx1 * q[0] + ry1 * q[1];
    let b = lerp(sx, u, v);
    lerp(sy, a, b)
}

fn random(seed: i32) -> i32 {
    let mut result = RAND_A * (seed % RAND_Q) - RAND_R * (seed / RAND_Q);
    if result <= 0 {
        result += RAND_M;
    }

    result
}

#[inline]
fn s_curve(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}
