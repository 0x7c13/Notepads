// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Original small SFNT mocks, not platform font bytes. Every mock has real glyf
//! outlines; optional COLR/SVG/sbix representations target only glyph 1.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
fn u16be(v: u16) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}
fn push16(v: &mut Vec<u8>, n: u16) {
    v.extend(n.to_be_bytes());
}
fn push32(v: &mut Vec<u8>, n: u32) {
    v.extend(n.to_be_bytes());
}
fn put16(v: &mut [u8], at: usize, n: u16) {
    v[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
pub(crate) fn sfnt(colr: bool, svg: bool, bitmap: bool) -> Vec<u8> {
    sfnt_with_bdt(colr, svg, bitmap, None)
}
fn sfnt_with_bdt(colr: bool, svg: bool, bitmap: bool, bdt: Option<u8>) -> Vec<u8> {
    let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![];
    let mut head = vec![0; 54];
    head[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
    put16(&mut head, 18, 1000);
    put16(&mut head, 40, 500);
    put16(&mut head, 42, 700);
    tables.push((*b"head", head));
    let mut maxp = 0x10000u32.to_be_bytes().to_vec();
    push16(&mut maxp, 3);
    maxp.resize(32, 0);
    tables.push((*b"maxp", maxp));
    let mut hhea = vec![0; 36];
    hhea[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    put16(&mut hhea, 4, 800);
    put16(&mut hhea, 6, (-200i16) as u16);
    put16(&mut hhea, 34, 3);
    tables.push((*b"hhea", hhea));
    let mut metrics = vec![];
    for _ in 0..3 {
        push16(&mut metrics, 600);
        push16(&mut metrics, 0);
    }
    tables.push((*b"hmtx", metrics));
    let mut glyph = vec![];
    for n in [1i16, 0, 0, 500, 700, 3, 0] {
        push16(&mut glyph, n as u16);
    }
    glyph.extend([1u8; 4]);
    for n in [0i16, 500, 0, -500, 0, 0, 700, 0] {
        push16(&mut glyph, n as u16);
    }
    assert_eq!(glyph.len(), 34);
    let mut glyf = glyph.clone();
    glyf.extend(&glyph);
    tables.push((*b"glyf", glyf));
    let mut loca = vec![];
    for n in [0, 0, 17, 34] {
        push16(&mut loca, n);
    }
    tables.push((*b"loca", loca));
    if colr {
        // v0 base glyph 1 -> outline glyph 2, palette entry 0.
        let mut data = u16be(0);
        push16(&mut data, 1);
        push32(&mut data, 14);
        push32(&mut data, 20);
        push16(&mut data, 1);
        for n in [1, 0, 1, 2, 0] {
            push16(&mut data, n);
        }
        tables.push((*b"COLR", data));
        let mut palette = vec![];
        for n in [0, 1, 1, 1] {
            push16(&mut palette, n);
        }
        push32(&mut palette, 14);
        push16(&mut palette, 0);
        palette.extend([0, 0, 255, 255]);
        tables.push((*b"CPAL", palette));
    }
    if svg {
        let doc=br#"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000" id="glyph1"><path d="M0 0H500V700H0Z" fill="red"/></svg>"#;
        let mut data = u16be(0);
        push32(&mut data, 10);
        push32(&mut data, 0);
        push16(&mut data, 1);
        push16(&mut data, 1);
        push16(&mut data, 1);
        push32(&mut data, 14);
        push32(&mut data, doc.len() as u32);
        data.extend(doc);
        tables.push((*b"SVG ", data));
    }
    if bitmap {
        // Original 1x1 PNG; header is enough for metadata and the ordinary glyph
        // conversion keeps these exact bytes for the normal decoder.
        let png: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 8, 215, 99, 248, 207,
            192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66,
            96, 130,
        ];
        let mut data = u16be(1);
        push16(&mut data, 3);
        push32(&mut data, 1);
        push32(&mut data, 12);
        push16(&mut data, 32);
        push16(&mut data, 72);
        for n in [20, 20, 28 + png.len() as u32, 28 + png.len() as u32] {
            push32(&mut data, n);
        }
        push16(&mut data, 0);
        push16(&mut data, 0);
        data.extend(b"png ");
        data.extend(png);
        tables.push((*b"sbix", data));
    }
    if let Some(bpp) = bdt {
        // Original 1x1 embedded glyph, indexFormat1/imageFormat1. No platform
        // bytes: the 1-bit EBDT mask and 32-bit CBDT BGRA differ only in kind.
        assert!(bpp == 1 || bpp == 32);
        let version = if bpp == 32 { 0x30000u32 } else { 0x20000u32 };
        let mut data = version.to_be_bytes().to_vec();
        data.extend([1, 1, 0, 1, 1]); // SmallGlyphMetrics(height,width,bearingX,Y,advance).
        if bpp == 32 {
            data.extend([0, 0, 255, 255]);
        } else {
            data.push(0x80);
        }
        let image_len = (data.len() - 4) as u32;
        let mut locations = version.to_be_bytes().to_vec();
        push32(&mut locations, 1);
        push32(&mut locations, 56);
        push32(&mut locations, 24);
        push32(&mut locations, 1);
        push32(&mut locations, 0);
        locations.extend([0; 24]); // Horizontal and vertical SbitLineMetrics.
        push16(&mut locations, 1);
        push16(&mut locations, 1);
        locations.extend([32, 32, bpp, 1]);
        assert_eq!(locations.len(), 56);
        push16(&mut locations, 1);
        push16(&mut locations, 1);
        push32(&mut locations, 8);
        push16(&mut locations, 1);
        push16(&mut locations, 1);
        push32(&mut locations, 4);
        push32(&mut locations, 0);
        push32(&mut locations, image_len);
        tables.push((if bpp == 32 { *b"CBDT" } else { *b"EBDT" }, data));
        tables.push((if bpp == 32 { *b"CBLC" } else { *b"EBLC" }, locations));
    }
    tables.sort_by_key(|(tag, _)| *tag);
    let mut result = 0x10000u32.to_be_bytes().to_vec();
    push16(&mut result, tables.len() as u16);
    result.resize(12, 0);
    let mut offset = 12 + tables.len() * 16;
    for (tag, data) in &tables {
        result.extend(tag);
        push32(&mut result, 0);
        push32(&mut result, offset as u32);
        push32(&mut result, data.len() as u32);
        offset += (data.len() + 3) & !3;
    }
    for (_, data) in tables {
        result.extend(data);
        while result.len() % 4 != 0 {
            result.push(0);
        }
    }
    result
}
#[derive(Debug)]
pub(super) struct Replay {
    db: Arc<Database>,
    synthetic: AtomicBool,
    id: ID,
}
#[derive(Debug)]
struct TestRetention;
impl ReplayRetention for TestRetention {}
impl FontReplay for Replay {
    fn reserve_replay_bytes(&self, _: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        // This legacy colour fixture does not claim memory-budget qualification.
        Ok(Arc::new(TestRetention))
    }
    fn fontdb(&self) -> Result<Arc<Database>, ()> {
        Ok(self.db.clone())
    }
    fn variations(&self, _: ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(Arc::from([]))
    }
    fn outline(&self, _: ID, _: u32) -> Result<ReplayOutline, ()> {
        Ok(if self.synthetic.load(Ordering::Relaxed) {
            ReplayOutline::Synthetic {
                data: None,
                rule: FillRule::NonZero,
            }
        } else {
            ReplayOutline::Ordinary
        })
    }
    fn prepare(&self, text: &Text, database: &mut Arc<Database>) -> Result<ReplayText, ()> {
        *database = self.db.clone();
        let face = ReplayFace {
            id: self.id,
            synthetic_metrics: Some(ReplayMetrics {
                units_per_em: 1000,
                ascent: 800,
                descent: -200,
                x_height: 500,
                underline_position: -100,
                underline_thickness: 50,
                strike_position: 300,
            }),
        };
        Ok(ReplayText {
            logical: None,
            chunks: text
                .chunks()
                .iter()
                .map(|chunk| {
                    chunk
                        .text()
                        .char_indices()
                        .map(|(at, ch)| ReplayGlyph {
                            identity: None,
                            start: at,
                            end: at + ch.len_utf8(),
                            glyph: 1,
                            face,
                            geometry_origin: ReplayGeometryOrigin::FontDesignOriginYUp,
                            x_offset: 0,
                            y_offset: 0,
                            advance: 600,
                            y_advance: 0,
                            fallback_scale: 1.0,
                        })
                        .collect::<Vec<_>>()
                        .into()
                })
                .collect::<Vec<_>>()
                .into(),
            styles: text
                .chunks()
                .iter()
                .map(|chunk| {
                    chunk
                        .spans()
                        .iter()
                        .map(|_| face)
                        .collect::<Vec<_>>()
                        .into()
                })
                .collect::<Vec<_>>()
                .into(),
        })
    }
}
fn mock(colr: bool, svg: bool, bitmap: bool, synthetic: bool) -> (Cache, Arc<Replay>, ID) {
    mock_data(sfnt(colr, svg, bitmap), synthetic)
}
pub(super) fn mock_data(data: Vec<u8>, synthetic: bool) -> (Cache, Arc<Replay>, ID) {
    let mut db = Database::new();
    let id = db.push_face_info(fontdb::FaceInfo {
        id: ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(data)),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    let db = Arc::new(db);
    let owner = Arc::new(Replay {
        db: db.clone(),
        synthetic: AtomicBool::new(synthetic),
        id,
    });
    let mut cache = Cache::new(db);
    cache.font_replay = Some(owner.clone());
    (cache, owner, id)
}
#[test]
fn synthetic_color_each_representation_and_mixed_outline_fail_before_consumption() {
    for (colr, svg, bitmap) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, true),
    ] {
        for entry in 0..3 {
            let (mut cache, _, id) = mock(colr, svg, bitmap, true);
            assert_eq!(
                cache
                    .fontdb
                    .has_color_glyph(id, GlyphId(1), &cache.processing)
                    .unwrap(),
                Some(true)
            );
            assert!(
                cache
                    .fontdb
                    .outline(id, GlyphId(1), &[], None, &cache.processing)
                    .is_some(),
                "actual colored glyph also has an outline"
            );
            match entry {
                0 => assert!(cache
                    .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                    .is_none()),
                1 => assert!(cache.fontdb_svg(id, GlyphId(1)).is_none()),
                _ => assert!(cache.fontdb_raster(id, GlyphId(1)).is_none()),
            }
            assert_eq!(
                cache.processing.failure(),
                Some(ProcessingFailure::FontReplay)
            );
            assert!(
                cache.cache_colr.is_empty()
                    && cache.cache_svg.is_empty()
                    && cache.cache_raster.is_empty()
            );
        }
    }
}

fn colr_foreground_paints(group: &Group) -> Vec<(Color, f32)> {
    let mut output = Vec::new();
    for node in group.children() {
        match node {
            Node::Group(group) => output.extend(colr_foreground_paints(group)),
            Node::Path(path) => {
                if let Some(fill) = path.fill() {
                    if let Paint::Color(color) = fill.paint() {
                        output.push((*color, fill.opacity().get()));
                    }
                }
            }
            _ => panic!("fixture must produce vector paths"),
        }
    }
    output
}

#[test]
fn colr_foreground_fixed_fixture_preserves_original_tables_except_palette_and_cmap() {
    fn tables(data: &[u8]) -> std::collections::BTreeMap<&[u8], Vec<u8>> {
        let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
        (0..count)
            .map(|index| {
                let at = 12 + index * 16;
                let offset = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap()) as usize;
                let length =
                    u32::from_be_bytes(data[at + 12..at + 16].try_into().unwrap()) as usize;
                (&data[at..at + 4], data[offset..offset + length].to_vec())
            })
            .collect()
    }
    let original = sfnt(true, false, false);
    let mut fixture = tables(include_bytes!("../../tests/fixtures/colr-foreground.ttf"));
    assert!(fixture.remove(b"cmap".as_slice()).is_some());
    let colr = fixture.get_mut(b"COLR".as_slice()).unwrap();
    assert_eq!(&colr[22..24], &[255, 255]);
    colr[22..24].copy_from_slice(&[0, 0]);
    assert_eq!(fixture, tables(&original));
}

#[test]
fn colr_foreground_cache_separates_rgb_and_alpha_and_keeps_palette_zero() {
    let data = include_bytes!("../../tests/fixtures/colr-foreground.ttf");
    let (mut cache, _, id) = mock_data(data.to_vec(), false);
    let red = svgtypes::Color::new_rgb(255, 0, 0);
    for color in [
        red,
        svgtypes::Color::new_rgb(0, 0, 255),
        red,
        svgtypes::Color::new_rgba(255, 0, 0, 128),
        svgtypes::Color::new_rgba(255, 0, 0, 64),
    ] {
        let tree = cache.fontdb_colr(id, GlyphId(1), &[], color).unwrap();
        assert_eq!(
            colr_foreground_paints(tree.root()),
            vec![(
                Color::new_rgb(color.red, color.green, color.blue),
                f32::from(color.alpha) / 255.0
            )]
        );
    }
    assert_eq!(cache.cache_colr.len(), 4);
    let (mut cache, _, id) = mock(true, false, false, false);
    for color in [
        svgtypes::Color::new_rgb(0, 0, 255),
        svgtypes::Color::new_rgba(0, 0, 255, 0),
    ] {
        let tree = cache.fontdb_colr(id, GlyphId(1), &[], color).unwrap();
        assert_eq!(
            colr_foreground_paints(tree.root()),
            vec![(Color::new_rgb(255, 0, 0), 1.0)]
        );
    }
}

#[test]
fn colr_foreground_populated_cache_cannot_bypass_synthetic_or_first_failure() {
    for synthetic in [false, true] {
        let (mut cache, owner, id) = mock(true, false, false, false);
        let color = svgtypes::Color::black();
        assert!(cache.fontdb_colr(id, GlyphId(1), &[], color).is_some());
        if synthetic {
            owner.synthetic.store(true, Ordering::Relaxed);
        } else {
            cache.processing.fail(ProcessingFailure::Cancelled);
        }
        assert!(cache.fontdb_colr(id, GlyphId(1), &[], color).is_none());
        assert_eq!(
            cache.processing.failure(),
            Some(if synthetic {
                ProcessingFailure::FontReplay
            } else {
                ProcessingFailure::Cancelled
            })
        );
        assert_eq!(cache.cache_colr.len(), 1);
    }
}
#[test]
fn ordinary_color_conversion_and_cache_keep_original_path() {
    let (mut cache, owner, id) = mock(true, true, true, false);
    for _ in 0..2 {
        assert!(cache
            .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
            .is_some());
        assert!(cache.fontdb_svg(id, GlyphId(1)).is_some());
        assert!(cache.fontdb_raster(id, GlyphId(1)).is_some());
        assert!(!cache.processing.failed());
    }
    // A populated ordinary cache cannot bypass the per-instance gate.
    owner.synthetic.store(true, Ordering::Relaxed);
    assert!(cache.fontdb_svg(id, GlyphId(1)).is_none());
    assert_eq!(
        cache.processing.failure(),
        Some(ProcessingFailure::FontReplay)
    );
}
#[test]
fn synthetic_outline_glyph_in_color_face_is_not_blanket_rejected() {
    let (mut cache, _, id) = mock(true, true, true, true);
    assert_eq!(
        cache
            .fontdb
            .has_color_glyph(id, GlyphId(2), &cache.processing)
            .unwrap(),
        Some(false)
    );
    assert!(cache
        .fontdb
        .outline(id, GlyphId(2), &[], None, &cache.processing)
        .is_some());
    assert!(cache
        .fontdb_colr(id, GlyphId(2), &[], svgtypes::Color::black())
        .is_none());
    assert!(cache.fontdb_svg(id, GlyphId(2)).is_none());
    assert!(cache.fontdb_raster(id, GlyphId(2)).is_none());
    assert!(!cache.processing.failed());
}
#[test]
fn synthetic_color_cannot_publish_partial_tree_as_ready() {
    for (colr, svg, bitmap) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, true),
    ] {
        let (cache, owner, _) = mock(colr, svg, bitmap, true);
        let options = Options {
            fontdb: cache.fontdb,
            font_replay: Some(owner),
            ..Options::default()
        };
        let parsed=Tree::from_data(br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><path d="M0 0H5V5Z"/><text x="10" y="50" font-family="Mock">A</text></svg>"#,&options);
        assert!(
            parsed.is_err(),
            "previous scene nodes cannot make failed color text Ready"
        );
    }
}

#[test]
fn synthetic_mask_alone_keeps_outline_but_mask_with_colr_or_svg_still_rejects() {
    use skrifa::MetadataProvider;
    for (colr, svg) in [(false, false), (true, false), (false, true)] {
        let (mut cache, _, id) = mock_data(sfnt_with_bdt(colr, svg, false, Some(1)), true);
        assert!(cache
            .fontdb
            .with_face_data(id, |data, index| {
                let font = skrifa::FontRef::from_index(data, index).unwrap();
                matches!(
                    font.bitmap_strikes()
                        .glyph_for_size(skrifa::prelude::Size::unscaled(), skrifa::GlyphId::new(1))
                        .unwrap()
                        .data,
                    skrifa::bitmap::BitmapData::Mask(_)
                )
            })
            .unwrap());
        assert_eq!(
            cache
                .fontdb
                .has_color_glyph(id, GlyphId(1), &cache.processing)
                .unwrap(),
            Some(colr || svg)
        );
        assert!(cache
            .fontdb
            .outline(id, GlyphId(1), &[], None, &cache.processing)
            .is_some());
        assert!(cache
            .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
            .is_none());
        if colr || svg {
            assert_eq!(
                cache.processing.failure(),
                Some(ProcessingFailure::FontReplay)
            );
        } else {
            assert!(cache.fontdb_svg(id, GlyphId(1)).is_none());
            assert!(cache.fontdb_raster(id, GlyphId(1)).is_none());
            assert!(!cache.processing.failed());
        }
    }
}
#[test]
fn synthetic_bgra_is_rejected_even_when_ordinary_raster_cannot_consume_it() {
    use skrifa::MetadataProvider;
    for entry in 0..3 {
        let (mut cache, _, id) = mock_data(sfnt_with_bdt(false, false, false, Some(32)), true);
        assert!(cache
            .fontdb
            .with_face_data(id, |data, index| {
                let font = skrifa::FontRef::from_index(data, index).unwrap();
                matches!(
                    font.bitmap_strikes()
                        .glyph_for_size(skrifa::prelude::Size::unscaled(), skrifa::GlyphId::new(1))
                        .unwrap()
                        .data,
                    skrifa::bitmap::BitmapData::Bgra(_)
                )
            })
            .unwrap());
        assert_eq!(
            cache
                .fontdb
                .has_color_glyph(id, GlyphId(1), &cache.processing)
                .unwrap(),
            Some(true)
        );
        match entry {
            0 => assert!(cache
                .fontdb_colr(id, GlyphId(1), &[], svgtypes::Color::black())
                .is_none()),
            1 => assert!(cache.fontdb_svg(id, GlyphId(1)).is_none()),
            _ => assert!(cache.fontdb_raster(id, GlyphId(1)).is_none()),
        }
        assert_eq!(
            cache.processing.failure(),
            Some(ProcessingFailure::FontReplay)
        );
    }
}
