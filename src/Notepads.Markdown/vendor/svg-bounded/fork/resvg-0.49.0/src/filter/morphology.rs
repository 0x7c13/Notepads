// Copyright 2020 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::ImageRefMut;
use rgb::RGBA8;
use usvg::filter::MorphologyOperator;

/// Applies a morphology filter.
///
/// `src` pixels should have a **premultiplied alpha**.
///
/// # Allocations
///
/// This method will allocate a copy of the `src` image as a back buffer.
pub fn apply(operator: MorphologyOperator, rx: f32, ry: f32, src: ImageRefMut) {
    let resources = src.resources.clone();
    let mut work = super::Work::new(resources.as_ref());
    let width_max = src.width - 1;
    let height_max = src.height - 1;
    let rx = (rx.round() as u32).min(width_max);
    let ry = (ry.round() as u32).min(height_max);

    let mut buf = match crate::raster::ScratchVec::try_filled(
        src.resources.as_ref(),
        src.data.len(),
        RGBA8::default(),
    ) {
        Ok(value) => value,
        Err(_) => return,
    };
    let mut buf = ImageRefMut::new(src.width, src.height, &mut buf, src.resources.clone());
    let mut x: u32 = 0;
    let mut y: u32 = 0;
    for _ in src.data.iter() {
        if !work.step() {
            return;
        }
        let mut new_p = RGBA8::default();
        if operator == MorphologyOperator::Erode {
            new_p.r = 255;
            new_p.g = 255;
            new_p.b = 255;
            new_p.a = 255;
        }

        let x_range = x.saturating_sub(rx)..=(x + rx).min(width_max);
        let y_range = y.saturating_sub(ry)..=(y + ry).min(height_max);

        for ty in y_range {
            if !work.step() {
                return;
            }
            for tx in x_range.clone() {
                if !work.step() {
                    return;
                }
                let p = src.pixel_at(tx, ty);
                if operator == MorphologyOperator::Erode {
                    new_p.r = p.r.min(new_p.r);
                    new_p.g = p.g.min(new_p.g);
                    new_p.b = p.b.min(new_p.b);
                    new_p.a = p.a.min(new_p.a);
                } else {
                    new_p.r = p.r.max(new_p.r);
                    new_p.g = p.g.max(new_p.g);
                    new_p.b = p.b.max(new_p.b);
                    new_p.a = p.a.max(new_p.a);
                }
            }
        }

        *buf.pixel_at_mut(x, y) = new_p;

        x += 1;
        if x == src.width {
            x = 0;
            y += 1;
        }
    }

    // Do not use `mem::swap` because `data` referenced via FFI.
    for (from, to) in buf.data.chunks(4096).zip(src.data.chunks_mut(4096)) {
        if let Some(r) = resources.as_ref() {
            if r.checkpoint(from.len() as u64).is_err() {
                return;
            }
        }
        to.copy_from_slice(from);
    }
}
