// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::painter::GeometryControl;
use core::convert::TryFrom;
use markdown_svg_budget::Error;
type RasterResult<T> = Result<T, Error>;

use crate::{FillRule, IntRect, LengthU32, Path, Rect};

use crate::alpha_runs::AlphaRuns;
use crate::blitter::Blitter;
use crate::color::AlphaU8;
use crate::geom::{IntRectExt, ScreenIntRect};
use crate::math::left_shift;

#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use tiny_skia_path::NoStdFloat;

/// controls how much we super-sample (when we use that scan conversion)
const SUPERSAMPLE_SHIFT: u32 = 2;

const SHIFT: u32 = SUPERSAMPLE_SHIFT;
const SCALE: u32 = 1 << SHIFT;
const MASK: u32 = SCALE - 1;

fn scan_discarded(path: &Path, control: GeometryControl) -> RasterResult<()> {
    // A clipped-out path may still contain arbitrarily many non-yielding verbs.
    let mut iter = crate::edge_builder::edge_iter(path);
    while iter.try_next(control)?.is_some() {}
    control.check(0)
}

pub(crate) fn fill_path_with_control(
    path: &Path,
    fill_rule: FillRule,
    clip: &ScreenIntRect,
    blitter: &mut dyn Blitter,
    control: GeometryControl,
) -> RasterResult<()> {
    control.check(0)?;
    // Unlike `path.bounds.to_rect()?.round_out()`,
    // this method rounds out first and then converts into a Rect.
    let ir = Rect::from_ltrb(
        path.bounds().left().floor(),
        path.bounds().top().floor(),
        path.bounds().right().ceil(),
        path.bounds().bottom().ceil(),
    )
    .and_then(|r| r.round_out());
    let ir = match ir {
        Some(v) => v,
        None => return scan_discarded(path, control),
    };

    // TODO: remove
    // If the intersection of the path bounds and the clip bounds
    // will overflow 32767 when << by SHIFT, we can't supersample,
    // so draw without antialiasing.
    let clipped_ir = match ir.intersect(&clip.to_int_rect()) {
        Some(v) => v,
        None => return scan_discarded(path, control),
    };
    if rect_overflows_short_shift(&clipped_ir, SHIFT as i32) != 0 {
        return super::path::fill_path_with_control(path, fill_rule, clip, blitter, control);
    }

    // TODO: remove
    // Our antialiasing can't handle a clip larger than 32767.
    // TODO: skia actually limits the clip to 32767
    {
        const MAX_CLIP_COORD: u32 = 32767;
        if clip.right() > MAX_CLIP_COORD || clip.bottom() > MAX_CLIP_COORD {
            return scan_discarded(path, control);
        }
    }

    // TODO: SkScanClipper
    // TODO: AAA

    fill_path_impl(path, fill_rule, &ir, clip, blitter, control)
}

// Would any of the coordinates of this rectangle not fit in a short,
// when left-shifted by shift?
fn rect_overflows_short_shift(rect: &IntRect, shift: i32) -> i32 {
    debug_assert!(overflows_short_shift(8191, shift) == 0);
    debug_assert!(overflows_short_shift(8192, shift) != 0);
    debug_assert!(overflows_short_shift(32767, 0) == 0);
    debug_assert!(overflows_short_shift(32768, 0) != 0);

    // Since we expect these to succeed, we bit-or together
    // for a tiny extra bit of speed.
    overflows_short_shift(rect.left(), shift)
        | overflows_short_shift(rect.top(), shift)
        | overflows_short_shift(rect.right(), shift)
        | overflows_short_shift(rect.bottom(), shift)
}

fn overflows_short_shift(value: i32, shift: i32) -> i32 {
    let s = 16 + shift;
    (left_shift(value, s) >> s) - value
}

fn fill_path_impl(
    path: &Path,
    fill_rule: FillRule,
    bounds: &IntRect,
    clip: &ScreenIntRect,
    blitter: &mut dyn Blitter,
    control: GeometryControl,
) -> RasterResult<()> {
    // TODO: MaskSuperBlitter

    // TODO: 15% slower than skia, find out why
    let mut blitter = match SuperBlitter::new(bounds, clip, blitter, control)? {
        Some(v) => v,
        None => return scan_discarded(path, control), // clipped out
    };

    let path_contained_in_clip = if let Some(bounds) = bounds.to_screen_int_rect() {
        clip.contains(&bounds)
    } else {
        // If bounds cannot be converted into ScreenIntRect,
        // the path is out of clip.
        false
    };

    let candidate = super::path::fill_path_impl_with_control(
        path,
        fill_rule,
        clip,
        bounds.top(),
        bounds.bottom(),
        SHIFT as i32,
        path_contained_in_clip,
        &mut blitter,
        control,
    );
    let candidate = control.finish(candidate).and_then(|()| blitter.finish());
    let candidate = control.finish(candidate);
    drop(blitter);
    control.finish(candidate)
}

struct BaseSuperBlitter<'a> {
    real_blitter: &'a mut dyn Blitter,

    /// Current y coordinate, in destination coordinates.
    curr_iy: i32,
    /// Widest row of region to be blitted, in destination coordinates.
    width: LengthU32,
    /// Leftmost x coordinate in any row, in destination coordinates.
    left: u32,
    /// Leftmost x coordinate in any row, in supersampled coordinates.
    super_left: u32,

    /// Current y coordinate in supersampled coordinates.
    curr_y: i32,
    /// Initial y coordinate (top of bounds).
    top: i32,
}

impl<'a> BaseSuperBlitter<'a> {
    fn new(
        bounds: &IntRect,
        clip_rect: &ScreenIntRect,
        blitter: &'a mut dyn Blitter,
    ) -> Option<Self> {
        let sect = bounds
            .intersect(&clip_rect.to_int_rect())?
            .to_screen_int_rect()?;
        Some(BaseSuperBlitter {
            real_blitter: blitter,
            curr_iy: sect.top() as i32 - 1,
            width: sect.width_safe(),
            left: sect.left(),
            super_left: sect.left() << SHIFT,
            curr_y: (sect.top() << SHIFT) as i32 - 1,
            top: sect.top() as i32,
        })
    }
}

struct SuperBlitter<'a, 'r> {
    control: GeometryControl<'r>,
    finished: bool,
    base: BaseSuperBlitter<'a>,
    runs: AlphaRuns,
    offset_x: usize,
}

impl<'a, 'r> SuperBlitter<'a, 'r> {
    fn new(
        bounds: &IntRect,
        clip_rect: &ScreenIntRect,
        blitter: &'a mut dyn Blitter,
        control: GeometryControl<'r>,
    ) -> RasterResult<Option<Self>> {
        let base = match BaseSuperBlitter::new(bounds, clip_rect, blitter) {
            Some(v) => v,
            None => return Ok(None),
        };
        let runs_width = base.width;
        Ok(Some(SuperBlitter {
            control,
            finished: false,
            base,
            runs: AlphaRuns::new_with_control(runs_width, control)?,
            offset_x: 0,
        }))
    }

    /// Once `runs` contains a complete supersampled row, flush() blits
    /// it out through the wrapped blitter.
    fn finish(&mut self) -> RasterResult<()> {
        self.flush()?;
        self.finished = true;
        self.control.check(0)
    }

    fn flush(&mut self) -> RasterResult<()> {
        self.control.check(0)?;
        if self.base.curr_iy >= self.base.top {
            if !self.runs.is_empty() {
                self.base.real_blitter.blit_anti_h(
                    self.base.left,
                    u32::try_from(self.base.curr_iy).unwrap(),
                    &mut self.runs.alpha,
                    &mut self.runs.runs,
                );
                self.control.check(0)?;
                self.runs.reset(self.base.width);
                self.offset_x = 0;
            }

            self.base.curr_iy = self.base.top - 1;
        }
        self.control.check(0)
    }
}

impl Drop for SuperBlitter<'_, '_> {
    fn drop(&mut self) {
        // Request paths only paint through explicit finish. An error or unwind
        // therefore discards the unfinished row; Drop never creates new pixels.
        if self.control.resources().is_none() && !self.finished {
            let _ = self.flush();
        }
    }
}

impl Blitter for SuperBlitter<'_, '_> {
    /// Blits a row of pixels, with location and width specified
    /// in supersampled coordinates.
    fn blit_h(&mut self, mut x: u32, y: u32, mut width: LengthU32) {
        if self.control.check(1).is_err() {
            return;
        }
        let iy = (y >> SHIFT) as i32;
        debug_assert!(iy >= self.base.curr_iy);

        // hack, until I figure out why my cubics (I think) go beyond the bounds
        match x.checked_sub(self.base.super_left) {
            Some(n) => x = n,
            None => {
                width = LengthU32::new(x + width.get()).unwrap();
                x = 0;
            }
        }

        debug_assert!(y as i32 >= self.base.curr_y);
        if self.base.curr_y != y as i32 {
            self.offset_x = 0;
            self.base.curr_y = y as i32;
        }

        if iy != self.base.curr_iy {
            // new scanline
            if self.flush().is_err() {
                return;
            }
            self.base.curr_iy = iy;
        }

        let start = x;
        let stop = x + width.get();

        debug_assert!(stop > start);
        // integer-pixel-aligned ends of blit, rounded out
        let mut fb = start & MASK;
        let mut fe = stop & MASK;
        let mut n: i32 = (stop as i32 >> SHIFT) - (start as i32 >> SHIFT) - 1;

        if n < 0 {
            fb = fe - fb;
            n = 0;
            fe = 0;
        } else {
            if fb == 0 {
                n += 1;
            } else {
                fb = SCALE - fb;
            }
        }

        let max_value = u8::try_from((1 << (8 - SHIFT)) - (((y & MASK) + 1) >> SHIFT)).unwrap();
        let offset = self.runs.add_with_control(
            x >> SHIFT,
            coverage_to_partial_alpha(fb),
            n as usize,
            coverage_to_partial_alpha(fe),
            max_value,
            self.offset_x,
            self.control,
        );
        if let Ok(offset) = offset {
            self.offset_x = offset;
        }
    }
}

// coverage_to_partial_alpha() is being used by AlphaRuns, which
// *accumulates* SCALE pixels worth of "alpha" in [0,(256/SCALE)]
// to produce a final value in [0, 255] and handles clamping 256->255
// itself, with the same (alpha - (alpha >> 8)) correction as
// coverage_to_exact_alpha().
fn coverage_to_partial_alpha(mut aa: u32) -> AlphaU8 {
    aa <<= 8 - 2 * SHIFT;
    aa as AlphaU8
}

#[cfg(test)]
#[path = "raster_control_tests.rs"]
mod raster_control_tests;
