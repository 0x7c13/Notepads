// Copyright 2020 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::{f32_bound, ImageRef, ImageRefMut};
use rgb::RGBA8;
use usvg::ApproxZeroUlps;

/// Performs an arithmetic composition.
///
/// - `src1` and `src2` image pixels should have a **premultiplied alpha**.
/// - `dest` image pixels will have a **premultiplied alpha**.
///
/// `src1`, `src2` and `dest` dimensions must match.
pub fn arithmetic(
    k1: f32,
    k2: f32,
    k3: f32,
    k4: f32,
    src1: ImageRef,
    src2: ImageRef,
    dest: ImageRefMut,
) {
    let resources = dest.resources.clone();
    let mut work = super::Work::new(resources.as_ref());
    debug_assert!(src1.width == src2.width && src1.width == dest.width);
    debug_assert!(src1.height == src2.height && src1.height == dest.height);

    let calc = |i1, i2, max| {
        let i1 = i1 as f32 / 255.0;
        let i2 = i2 as f32 / 255.0;
        let result = k1 * i1 * i2 + k2 * i1 + k3 * i2 + k4;
        f32_bound(0.0, result, max)
    };

    let mut i = 0;
    for (c1, c2) in src1.data.iter().zip(src2.data.iter()) {
        if !work.step() {
            return;
        }
        let a = calc(c1.a, c2.a, 1.0);
        if a.approx_zero_ulps(4) {
            i += 1;
            continue;
        }

        let r = (calc(c1.r, c2.r, a) * 255.0) as u8;
        let g = (calc(c1.g, c2.g, a) * 255.0) as u8;
        let b = (calc(c1.b, c2.b, a) * 255.0) as u8;
        let a = (a * 255.0) as u8;

        dest.data[i] = RGBA8 { r, g, b, a };

        i += 1;
    }
}
