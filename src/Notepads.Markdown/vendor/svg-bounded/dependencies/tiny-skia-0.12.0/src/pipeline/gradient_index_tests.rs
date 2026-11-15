// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use crate::{
    BlendMode, Color, GradientStop, LinearGradient, Paint, Pixmap, Point, Rect, SpreadMode,
    Transform,
};
use alloc::vec;
use std::eprintln;

pub(crate) fn assert_last_stop_color(
    stop_count: usize,
    width: u32,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) {
    let stops = (0..stop_count)
        .map(|i| {
            // Keep the factory's leading/trailing constant-color entries: both
            // endpoint colors must differ from their neighboring stop.
            let color = if i == 0 {
                Color::from_rgba8(255, 0, 0, 255)
            } else if i + 1 == stop_count {
                Color::WHITE
            } else {
                Color::BLACK
            };
            GradientStop::new(i as f32 / (stop_count - 1) as f32, color)
        })
        .collect();
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(0.5, 0.0),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();
    let paint = Paint {
        shader,
        anti_alias: false,
        force_hq_pipeline: false,
        blend_mode: BlendMode::Source,
        ..Paint::default()
    };
    let mut pixmap = Pixmap::new(width, 1).unwrap();
    let rect = Rect::from_xywh(0.0, 0.0, width as f32, 1.0).unwrap();
    match resources {
        None => pixmap.fill_rect(rect, &paint, Transform::identity(), None),
        Some(r) => pixmap
            .try_fill_rect(rect, &paint, Transform::identity(), None, r)
            .unwrap(),
    }
    eprintln!(
        "stops={stop_count}, width={width}, rgba={:?}",
        pixmap.data()
    );
    // Pixel centers are at or beyond the gradient endpoint. Pad therefore
    // selects the exact final opaque white, independent of interpolation.
    assert_eq!(pixmap.data(), vec![255u8; width as usize * 4]);
}

#[test]
fn short_gradient_endpoint() {
    assert_last_stop_color(3, 17, None);
}

#[test]
fn last_u16_index_endpoint() {
    assert_last_stop_color(65_535, 17, None);
}

#[test]
fn first_wide_index_tail() {
    assert_last_stop_color(65_536, 1, None);
}

#[test]
fn first_wide_index_full_vector() {
    assert_last_stop_color(65_536, 16, None);
}

#[test]
fn first_wide_index_full_and_tail() {
    assert_last_stop_color(65_536, 17, None);
}

#[test]
fn beyond_u16_index_endpoint() {
    assert_last_stop_color(65_537, 17, None);
}
