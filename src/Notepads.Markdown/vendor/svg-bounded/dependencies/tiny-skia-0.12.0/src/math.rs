// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::LengthU32;

#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use tiny_skia_path::NoStdFloat;

// Perfectly safe.
pub const LENGTH_U32_ONE: LengthU32 = unsafe { LengthU32::new_unchecked(1) };

pub fn left_shift(value: i32, shift: i32) -> i32 {
    ((value as u32) << shift) as i32
}

pub fn left_shift64(value: i64, shift: i32) -> i64 {
    ((value as u64) << shift) as i64
}

pub fn bound<T: Ord + Copy>(min: T, value: T, max: T) -> T {
    max.min(value).max(min)
}

// Skia cites http://www.machinedlearnings.com/2011/06/fast-approximate-logarithm-exponential.html
pub fn approx_powf(x: f32, y: f32) -> f32 {
    if x == 0.0 || x == 1.0 {
        return x;
    }

    let e = x.to_bits() as f32 * (1.0f32 / ((1 << 23) as f32));
    let m = f32::from_bits((x.to_bits() & 0x007fffff) | 0x3f000000);

    let log2_x = e - 124.225514990f32 - 1.498030302f32 * m - 1.725879990f32 / (0.3520887068f32 + m);

    let x = log2_x * y;

    let f = x - x.floor();

    let mut a = x + 121.274057500f32;
    a -= f * 1.490129070f32;
    a += 27.728023300f32 / (4.84252568f32 - f);
    a *= (1 << 23) as f32;

    if a < f32::INFINITY.to_bits() as f32 {
        if a > 0.0 {
            f32::from_bits(a.round() as u32)
        } else {
            0.0
        }
    } else {
        f32::INFINITY
    }
}
