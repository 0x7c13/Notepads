// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::{
    control::test_support::*,
    instance::{LocationRef, Size},
    setting::VariationSetting,
    MetadataProvider,
};
use markdown_svg_budget::{Limits, RequestControl};
use read_fonts::{
    types::{F2Dot14, Tag},
    TableProvider,
};

fn axes_font(count: u16) -> Vec<u8> {
    let mut fvar = Vec::new();
    for n in [1u16, 0, 16, 2, count, 20, 0, 4 + count * 4] {
        fvar.extend(n.to_be_bytes());
    }
    for index in 0..count {
        let mut tag = *b"ax00";
        tag[3] += index as u8;
        fvar.extend(tag);
        for fixed in [0i32, 0, 0x10000] {
            fvar.extend(fixed.to_be_bytes());
        }
        fvar.extend(0u16.to_be_bytes());
        fvar.extend((256 + index).to_be_bytes());
    }
    sfnt(vec![(*b"fvar", fvar)])
}

#[test]
fn axes_location_inline8_and_ninth_spill_keep_the_real_owner() {
    for count in [8u16, 9] {
        let bytes = axes_font(count);
        let font = FontRef::new(&bytes).unwrap();
        let (r, events) = resources(None, Limits::default());
        let axes = Axes::try_new(&font, &r).unwrap();
        assert_eq!(axes.len(), count as usize);
        let last = if count == 8 {
            Tag::new(b"ax07")
        } else {
            Tag::new(b"ax08")
        };
        let settings = [
            VariationSetting::new(Tag::new(b"ax00"), 0.75),
            VariationSetting::new(last, 1.0),
            VariationSetting::new(Tag::new(b"xxxx"), 7.0),
        ];
        let expected = font.axes().location(settings);
        let location = axes.try_location(&settings).unwrap();
        assert_eq!(location.coords(), expected.coords());
        assert_eq!(location.coords()[0].to_bits(), 12288);
        assert_eq!(location.coords()[count as usize - 1].to_bits(), 16384);
        assert!(location.belongs_to(&r));
        let (other, _) = resources(None, Limits::default());
        assert!(!location.belongs_to(&other));
        assert_eq!(
            axes.try_get_by_tag(last).unwrap().unwrap().index(),
            count as usize - 1
        );
        assert!(axes.try_get(count as usize).unwrap().is_none());
        if count == 8 {
            assert_eq!(r.statistics().live_leases, 0);
            assert_eq!(events.lock().unwrap().reserve, 0);
        } else {
            assert_eq!(r.statistics().live_leases, 1);
            assert_eq!(r.statistics().scratch_bytes, 18);
        }
        drop(location);
        assert_eq!(r.statistics().scratch_bytes, 0);
        r.validate_retired().unwrap();
    }
    let bytes = axes_font(9);
    let font = FontRef::new(&bytes).unwrap();
    let (r, events) = resources(Some(Fault::Release(1)), Limits::default());
    let axes = Axes::try_new(&font, &r).unwrap();
    let location = axes.try_location(&[]).unwrap();
    assert_eq!(r.failure(), None);
    assert_eq!(r.statistics().live_leases, 1);
    drop(location);
    assert_eq!(r.failure(), Some(FAULT));
    assert_eq!(events.lock().unwrap().injected, 1);
    assert_eq!(axes.try_get(0).err(), Some(FAULT));
    assert_eq!(r.validate_retired(), Err(FAULT));
}

fn cmap12(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(12u16.to_be_bytes());
    bytes.extend(0u16.to_be_bytes());
    bytes.extend((16u32 + groups.len() as u32 * 12).to_be_bytes());
    bytes.extend(0u32.to_be_bytes());
    bytes.extend((groups.len() as u32).to_be_bytes());
    for &(first, last, glyph) in groups {
        for n in [first, last, glyph] {
            bytes.extend(n.to_be_bytes());
        }
    }
    bytes
}
fn cmap_font(records: Vec<(u16, u16, Vec<u8>)>) -> Vec<u8> {
    let mut cmap = vec![0; 4 + records.len() * 8];
    cmap[2..4].copy_from_slice(&(records.len() as u16).to_be_bytes());
    for (index, (platform, encoding, table)) in records.into_iter().enumerate() {
        let offset = cmap.len() as u32;
        let start = 4 + index * 8;
        cmap[start..start + 2].copy_from_slice(&platform.to_be_bytes());
        cmap[start + 2..start + 4].copy_from_slice(&encoding.to_be_bytes());
        cmap[start + 4..start + 8].copy_from_slice(&offset.to_be_bytes());
        cmap.extend(table);
    }
    sfnt(vec![(*b"cmap", cmap)])
}

#[test]
fn symbol_notdef_pua_and_reverse_equal_priority_keep_sk_selection() {
    for symbol in [false, true] {
        let mut records = vec![
            (0, 4, cmap12(&[(65, 65, 3)])),
            (3, 1, cmap12(&[(65, 65, 2)])),
            (0, 4, cmap12(&[(65, 65, 4)])),
        ];
        if symbol {
            records.insert(0, (3, 0, cmap12(&[(65, 65, 0), (0xF041, 0xF041, 7)])));
        }
        let bytes = cmap_font(records);
        let font = FontRef::new(&bytes).unwrap();
        let (r, _) = resources(None, Limits::default());
        let map = Charmap::try_new(&font, &r).unwrap();
        let plain = font.charmap();
        assert_eq!(map.is_symbol(), symbol);
        assert_eq!(
            map.try_map(65).unwrap(),
            Some(GlyphId::new(if symbol { 7 } else { 4 }))
        );
        for ch in [0, 65, 66, 0xF041, u32::MAX] {
            assert_eq!(map.try_map(ch).unwrap(), plain.map(ch));
        }
        r.validate_retired().unwrap();
    }
    let font = FontRef::new(font_test_data::CMAP14_FONT1).unwrap();
    let plain = font.charmap();
    let (r, _) = resources(None, Limits::default());
    let map = Charmap::try_new(&font, &r).unwrap();
    for ch in [0, 0x4E00, 0x4E01, 0x537F, 0x908A, u32::MAX] {
        for selector in [0xFE00, 0xFE01, 0xE0100, u32::MAX] {
            assert_eq!(
                map.try_map_variant(ch, selector).unwrap(),
                plain.map_variant(ch, selector)
            );
        }
    }
    r.validate_retired().unwrap();
}

#[test]
fn long_zero_coords_and_ignored_encoding_records_stop_in_the_second_window() {
    let coords = vec![F2Dot14::ZERO; 513];
    let (r, _) = resources(None, Limits::default());
    assert!(LocationRef::new(&coords)
        .try_effective_coords(&mut RequestControl(&r))
        .unwrap()
        .is_empty());
    assert_eq!(r.statistics().work, 513);
    for nonzero in [false, true] {
        let mut coords = coords.clone();
        if nonzero {
            coords[256] = F2Dot14::from_bits(1);
        }
        let (r, events) = resources(Some(Fault::Work(257)), Limits::default());
        let location = LocationRef::new(&coords);
        assert_eq!(
            location.try_effective_coords(&mut RequestControl(&r)).err(),
            Some(FAULT)
        );
        assert_eq!(events.lock().unwrap().positive_work, 257);
        let calls = events.lock().unwrap().work.len();
        assert_eq!(
            location.try_effective_coords(&mut RequestControl(&r)).err(),
            Some(FAULT)
        );
        assert_eq!(events.lock().unwrap().work.len(), calls);
        r.validate_retired().unwrap();
    }
    // Macintosh records are intentionally ignored by SK's Unicode strategy;
    // no subtable read can accidentally supply the missing per-record work.
    let bytes = cmap_font((0..513).map(|_| (1, 0, cmap12(&[(65, 65, 1)]))).collect());
    let font = FontRef::new(&bytes).unwrap();
    let (r, events) = resources(Some(Fault::Work(256)), Limits::default());
    assert_eq!(Charmap::try_new(&font, &r).err(), Some(FAULT));
    assert_eq!(events.lock().unwrap().injected, 1);
    let calls = events.lock().unwrap().work.len();
    assert_eq!(Charmap::try_new(&font, &r).err(), Some(FAULT));
    assert_eq!(events.lock().unwrap().work.len(), calls);
    r.validate_retired().unwrap();
}

fn box_bits(value: Option<crate::metrics::BoundingBox>) -> Vec<u32> {
    match value {
        None => vec![0],
        Some(value) => vec![
            1,
            value.x_min.to_bits(),
            value.y_min.to_bits(),
            value.x_max.to_bits(),
            value.y_max.to_bits(),
        ],
    }
}
fn metric_bits(m: crate::metrics::Metrics) -> Vec<u32> {
    let mut values = vec![
        m.units_per_em as u32,
        m.glyph_count as u32,
        m.is_monospace as u32,
        m.italic_angle.to_bits(),
        m.ascent.to_bits(),
        m.descent.to_bits(),
        m.leading.to_bits(),
    ];
    for field in [m.cap_height, m.x_height, m.average_width, m.max_width] {
        match field {
            None => values.push(0),
            Some(value) => values.extend([1, value.to_bits()]),
        }
    }
    for field in [m.underline, m.strikeout] {
        match field {
            None => values.push(0),
            Some(value) => values.extend([1, value.offset.to_bits(), value.thickness.to_bits()]),
        }
    }
    values.extend(box_bits(m.bounds));
    values
}
#[test]
fn all_copy_metrics_and_static_variable_cff_bounds_match_original_bits() {
    for data in [
        font_test_data::SIMPLE_GLYF,
        font_test_data::VAZIRMATN_VAR,
        font_test_data::CANTARELL_VF_TRIMMED,
        font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
    ] {
        let font = FontRef::new(data).unwrap();
        let count = font.maxp().unwrap().num_glyphs() as u32;
        for size in [Size::unscaled(), Size::new(17.5)] {
            for value in [0.0, 0.5] {
                let coords = [F2Dot14::from_f32(value); 3];
                let location = LocationRef::new(&coords);
                let (r, _) = resources(None, Limits::default());
                let plain = font.metrics(size, location);
                let checked = try_metrics(&font, size, location, &r).unwrap();
                assert_eq!(metric_bits(plain), metric_bits(checked));
                let expected = font.glyph_metrics(size, location);
                let actual = GlyphMetrics::try_new(&font, size, location, &r).unwrap();
                for id in 0..=count {
                    let id = GlyphId::new(id);
                    assert_eq!(
                        box_bits(expected.bounds(id)),
                        box_bits(actual.try_bounds(id).unwrap()),
                        "bbox {:?}",
                        id
                    );
                }
                assert_eq!(r.failure(), None);
                r.validate_retired().unwrap();
            }
        }
    }
}
