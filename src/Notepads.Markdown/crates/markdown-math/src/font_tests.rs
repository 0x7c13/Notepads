// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use std::sync::atomic::AtomicBool;

use ratex_types::{
    Color, DisplayItem, DisplayList, PathCommand,
    execution::{ExecutionControl, ExecutionError, Limits, Phase, Resource},
    storage::{FAMILY_BYTES, StorageError, StorageFamily, StorageLedger},
};

use crate::{
    fonts::{FONT_BYTES, FONT_COUNT, FontAssets, FontLookupError, UNKNOWN_FONT},
    outline::outline_drawing,
    types::{InkBounds, MathArtifactData, MathError, MathPathCommand},
};

fn drawing(items: Vec<DisplayItem>) -> DisplayList {
    DisplayList {
        items,
        width: 2.0,
        height: 1.0,
        depth: 0.25,
    }
}

fn glyph(font: &str, codepoint: u32) -> DisplayItem {
    DisplayItem::GlyphPath {
        x: 0.0,
        y: 1.0,
        scale: 1.0,
        font: font.to_owned(),
        char_code: codepoint,
        color: Color::BLACK,
    }
}

fn convert(display: &DisplayList) -> Result<MathArtifactData, MathError> {
    let cancelled = AtomicBool::new(false);
    let control = ExecutionControl::new(&cancelled, None, Limits::default());
    let storage = StorageLedger::new(&control);
    outline_drawing(display, &FontAssets::new(), &storage)
}

#[test]
fn every_fixed_font_parses_as_a_borrowed_face() {
    let assets = FontAssets::new();
    assert_eq!(assets.len(), FONT_COUNT);
    assert_eq!(assets.byte_len(), FONT_BYTES);
    assert_eq!(assets.asset_set(), "katex-ttf-20-v1");
    for index in 0..FONT_COUNT {
        let name = assets.name(index).expect("fixed font name");
        let face = assets.face(name).expect("fixed font parses");
        assert_eq!(face.index(), index as u16);
        assert!(face.face().units_per_em() > 0);
        assert!(face.face().number_of_glyphs() > 0);
    }
    assert!(assets.name(FONT_COUNT).is_none());
}

#[test]
fn mathematical_alphanumeric_cmap_uses_the_original_font_mapping() {
    let assets = FontAssets::new();
    let bold = assets.face("Main-Bold").expect("bold face");
    assert_eq!(
        bold.glyph(0x1d400).unwrap(),
        bold.glyph(u32::from(b'A')).unwrap()
    );
    let regular = assets.face("Main-Regular").expect("regular face");
    assert!(matches!(
        regular.glyph(0x1d400),
        Err(FontLookupError::MissingGlyph { .. })
    ));
}

#[test]
fn invalid_scalar_is_not_replaced_by_replacement_character() {
    let assets = FontAssets::new();
    let face = assets.face("Main-Regular").unwrap();
    assert_eq!(
        face.glyph(0x11_0000),
        Err(FontLookupError::InvalidCodepoint {
            font: face.index(),
            codepoint: 0x11_0000,
        })
    );
}

#[test]
fn actual_glyph_is_a_nonempty_owned_numeric_outline() {
    let output = convert(&drawing(vec![glyph("Math-Italic", u32::from(b'x'))])).unwrap();
    assert_eq!(output.metrics.advance, 2.0);
    assert_eq!(output.metrics.ascent, 1.0);
    assert_eq!(output.metrics.descent, 0.25);
    assert_eq!(output.primitives.len(), 1);
    assert!(output.primitives[0].fill);
    assert_eq!(
        output.primitives[0].command_len as usize,
        output.commands.len()
    );
    assert!(!output.commands.is_empty());
    assert!(output.metrics.ink.bottom <= 1.1);
    assert!(output.metrics.ink.top < 1.0);
    assert!(output.metrics.ink.right > output.metrics.ink.left);
}

#[test]
fn a_present_space_glyph_can_have_no_ink() {
    let output = convert(&drawing(vec![glyph("Main-Regular", u32::from(b' '))])).unwrap();
    assert!(output.primitives.is_empty());
    assert!(output.commands.is_empty());
    assert_eq!(output.metrics.ink, InkBounds::default());
}

#[test]
fn absent_glyph_and_unknown_font_are_visible_typed_failures() {
    assert!(matches!(
        convert(&drawing(vec![glyph("Main-Regular", 0x4e2d)])),
        Err(MathError::MissingGlyph {
            codepoint: 0x4e2d,
            ..
        })
    ));
    assert!(matches!(
        convert(&drawing(vec![glyph("CJK-Regular", 0x4e2d)])),
        Err(MathError::MissingFont { font: UNKNOWN_FONT })
    ));
}

#[test]
fn paths_are_translated_numerically_and_keep_their_paint() {
    let output = convert(&drawing(vec![DisplayItem::Path {
        x: 3.0,
        y: 5.0,
        fill: true,
        color: Color::rgb(1.0, 0.0, 0.0),
        commands: vec![
            PathCommand::MoveTo { x: 1.0, y: 2.0 },
            PathCommand::QuadTo {
                x1: 2.0,
                y1: -3.0,
                x: 4.0,
                y: 6.0,
            },
            PathCommand::Close,
        ],
    }]))
    .unwrap();
    assert_eq!(
        output.commands,
        [
            MathPathCommand::MoveTo { x: 4.0, y: 7.0 },
            MathPathCommand::QuadTo {
                x1: 5.0,
                y1: 2.0,
                x: 7.0,
                y: 11.0
            },
            MathPathCommand::Close,
        ]
    );
    assert_eq!(
        output.metrics.ink,
        InkBounds {
            left: 4.0,
            top: 2.0,
            right: 7.0,
            bottom: 11.0
        }
    );
    assert_eq!(output.primitives[0].color, Color::rgb(1.0, 0.0, 0.0));
}

#[test]
fn dashed_lines_have_finite_explicit_segments() {
    let output = convert(&drawing(vec![DisplayItem::Line {
        x: 0.0,
        y: 0.5,
        width: 1.0,
        thickness: 0.04,
        color: Color::BLACK,
        dashed: true,
    }]))
    .unwrap();
    assert_eq!(output.primitives.len(), 1);
    assert!(!output.primitives[0].fill);
    assert_eq!(output.primitives[0].stroke_width, 0.04);
    assert_eq!(output.commands.len(), 8);
    assert!(matches!(
        output.commands[7],
        MathPathCommand::LineTo { x: 1.0, y: 0.5 }
    ));
    assert_eq!(output.metrics.ink.top, 0.48);
    assert_eq!(output.metrics.ink.bottom, 0.52);
}

#[test]
fn tiny_dashes_cannot_create_an_unbounded_loop() {
    assert!(matches!(
        convert(&drawing(vec![DisplayItem::Line {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            thickness: 1e-20,
            color: Color::BLACK,
            dashed: true,
        }])),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Outline,
            resource: Resource::OutlineSegments,
            limit: 65_536,
            ..
        }))
    ));
}

#[test]
fn invalid_geometry_or_color_never_produces_a_batch() {
    assert!(matches!(
        convert(&drawing(vec![DisplayItem::Rect {
            x: f64::NAN,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            color: Color::BLACK,
        }])),
        Err(MathError::InvalidGeometry)
    ));
    assert!(matches!(
        convert(&drawing(vec![DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            color: Color::new(1.0, 0.0, 0.0, f32::NAN),
        }])),
        Err(MathError::InvalidGeometry)
    ));
    assert!(matches!(
        convert(&drawing(vec![DisplayItem::Path {
            x: 0.0,
            y: 0.0,
            fill: true,
            color: Color::BLACK,
            commands: vec![PathCommand::LineTo { x: 1.0, y: 1.0 }],
        }])),
        Err(MathError::InvalidGeometry)
    ));
}

#[test]
fn quota_failure_in_a_real_glyph_callback_discards_the_batch() {
    let cancelled = AtomicBool::new(false);
    let mut limits = Limits::default();
    limits.values[Resource::OutlineSegments as usize] = 2;
    let control = ExecutionControl::new(&cancelled, None, limits);
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        outline_drawing(
            &drawing(vec![glyph("Math-Italic", u32::from(b'x'))]),
            &FontAssets::new(),
            &storage,
        ),
        Err(MathError::Execution(ExecutionError::ResourceLimit {
            phase: Phase::Outline,
            resource: Resource::OutlineSegments,
            limit: 2,
            requested: 3,
        }))
    ));
    assert_eq!(
        control.consumed_counts()[Resource::OutlineSegments as usize],
        2
    );
}

#[test]
fn path_capacity_is_rejected_before_an_unfunded_append() {
    let cancelled = AtomicBool::new(false);
    let control = ExecutionControl::new(&cancelled, None, Limits::default());
    let storage = StorageLedger::new(&control);
    storage
        .charge(
            Phase::Outline,
            StorageFamily::Path,
            FAMILY_BYTES[StorageFamily::Path as usize],
            0,
        )
        .unwrap();
    assert!(matches!(
        outline_drawing(
            &drawing(vec![glyph("Math-Italic", u32::from(b'x'))]),
            &FontAssets::new(),
            &storage,
        ),
        Err(MathError::Storage(StorageError::FamilyBytes {
            family: StorageFamily::Path,
            ..
        }))
    ));
}

#[test]
fn already_cancelled_work_returns_before_font_scanning() {
    let cancelled = AtomicBool::new(true);
    let control = ExecutionControl::new(&cancelled, None, Limits::default());
    let storage = StorageLedger::new(&control);
    assert!(matches!(
        outline_drawing(
            &drawing(vec![glyph("Math-Italic", u32::from(b'x'))]),
            &FontAssets::new(),
            &storage,
        ),
        Err(MathError::Execution(ExecutionError::Cancelled {
            phase: Phase::Outline
        }))
    ));
}
