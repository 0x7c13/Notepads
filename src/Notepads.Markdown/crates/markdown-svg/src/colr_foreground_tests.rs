// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;
use usvg::fontdb;

#[test]
fn colr_foreground_actual_pixels_keep_rgba_and_background_composition() {
    let mut database = fontdb::Database::new();
    database.push_face_info(fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(
            include_bytes!(
                "../../../vendor/svg-bounded/fork/usvg-0.49.0/tests/fixtures/colr-foreground.ttf"
            )
            .to_vec(),
        )),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    let mut options = usvg::Options::default();
    options.fontdb = Arc::new(database);
    // Rectangles occupy x=[10,20],[40,50],[70,80],[100,110], y=[16,30].
    // Interior pixels have full coverage; expected bytes are independent
    // premultiplied source colors and source-over opaque-white results.
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="130" height="50"><text y="30" font-family="Mock" font-size="20" fill="lime"><tspan x="10" color="red">A</tspan><tspan x="40" color="blue">A</tspan><tspan x="70" color="rgba(255,0,0,0.5)">A</tspan><tspan x="100" color="rgba(255,0,0,0)">A</tspan></text></svg>"#;
    let tree = usvg::Tree::from_data(source, &options).unwrap();
    for white in [false, true] {
        let mut pixels = tiny_skia::Pixmap::new(130, 50).unwrap();
        if white {
            pixels.fill(tiny_skia::Color::WHITE);
        }
        resvg::render_in_request(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
        )
        .unwrap();
        let expected = if white {
            [
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [255, 127, 127, 255],
                [255, 255, 255, 255],
            ]
        } else {
            [
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [128, 0, 0, 128],
                [0, 0, 0, 0],
            ]
        };
        for (x, expected) in [15, 45, 75, 105].into_iter().zip(expected) {
            let actual = pixels.pixel(x, 23).unwrap();
            assert_eq!(
                [actual.red(), actual.green(), actual.blue(), actual.alpha()],
                expected,
                "x={x}, white={white}"
            );
        }
    }
}

fn mixed_colr_outline_font() -> Vec<u8> {
    let mut data = include_bytes!(
        "../../../vendor/svg-bounded/fork/usvg-0.49.0/tests/fixtures/colr-foreground.ttf"
    )
    .to_vec();
    // The original fixture maps A to COLRv0 glyph 1. Extend its existing
    // format12 group by one codepoint so B maps to ordinary outline glyph 2;
    // all outlines, COLR/CPAL payloads, metrics and table offsets stay original.
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    let record = (0..count)
        .map(|index| 12 + index * 16)
        .find(|&at| &data[at..at + 4] == b"cmap")
        .unwrap();
    let cmap = u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize;
    let encodings = u16::from_be_bytes(data[cmap + 2..cmap + 4].try_into().unwrap()) as usize;
    let subtable = (0..encodings)
        .map(|index| {
            let at = cmap + 4 + index * 8;
            cmap + u32::from_be_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize
        })
        .find(|&at| u16::from_be_bytes(data[at..at + 2].try_into().unwrap()) == 12)
        .unwrap();
    let word = |at| u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
    assert_eq!(word(subtable + 12), 1);
    assert_eq!(word(subtable + 16), u32::from('A'));
    assert_eq!(word(subtable + 20), u32::from('A'));
    assert_eq!(word(subtable + 24), 1);
    data[subtable + 20..subtable + 24].copy_from_slice(&u32::from('B').to_be_bytes());
    data
}

fn assert_colr_overlap_pixel(content: &str, expected_glyphs: [u32; 2], expected: [u8; 4]) {
    let mut database = fontdb::Database::new();
    let font = database.push_face_info(fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(mixed_colr_outline_font())),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    let mut options = usvg::Options::default();
    options.fontdb = Arc::new(database);
    assert!(
        options.font_replay.is_none(),
        "exercise actual ordinary HarfRust shaping"
    );
    // 600 font units advance at 20em is 12 DIP. A relative second dx=-12
    // overlaps the glyphs without the chunk boundary of an absolute x list.
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="50"><text id="overlap" x="10" y="30" dx="0 -12" font-family="Mock" font-size="20" color="red" fill="black">{content}</text></svg>"#
    );
    let tree = usvg::Tree::from_data(source.as_bytes(), &options).unwrap();
    let text = match tree.node_by_id("overlap").unwrap() {
        usvg::Node::Text(text) => text,
        _ => panic!("expected the real Text node"),
    };
    assert_eq!(text.chunks().len(), 1);
    assert_eq!(text.layouted().len(), 1);
    let glyphs = text.layouted()[0].positioned_glyphs();
    assert_eq!(glyphs.len(), 2);
    assert_eq!([glyphs[0].id.0, glyphs[1].id.0], expected_glyphs);
    assert!(glyphs.iter().all(|glyph| glyph.font == font));
    let bits = |matrix: tiny_skia::Transform| {
        [
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
        ]
        .map(f32::to_bits)
    };
    assert_eq!(
        bits(glyphs[0].outline_transform()),
        bits(glyphs[1].outline_transform()),
        "the real placed outlines must overlap before judging painter order"
    );
    let mut pixels = tiny_skia::Pixmap::new(40, 50).unwrap();
    resvg::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
    )
    .unwrap();
    // The original glyph rectangle occupies x=[10,20], y=[16,30]. This is a
    // fully covered interior point, with independent opaque red/black answers.
    let actual = pixels.pixel(15, 23).unwrap();
    assert_eq!(
        [actual.red(), actual.green(), actual.blue(), actual.alpha()],
        expected,
        "content={content}: actual mixed glyph painter order"
    );
    assert_eq!(pixels.pixel(0, 0).unwrap().alpha(), 0);
}

#[test]
fn colr_overlap_actual_harfrust_ordinary_then_color_finishes_red() {
    assert_colr_overlap_pixel("BA", [2, 1], [255, 0, 0, 255]);
}

#[test]
fn colr_overlap_actual_harfrust_color_then_ordinary_finishes_black() {
    assert_colr_overlap_pixel("AB", [1, 2], [0, 0, 0, 255]);
}
