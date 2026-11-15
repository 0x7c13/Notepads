// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual representation reader transport, search and parse-before-filter tests.
use crate::{
    control::{TryFontRead, TryFontReadWithArgs},
    tables::{bitmap, colr, svg},
    FontData, FontRead, ReadError,
};
use markdown_svg_budget::WorkControl;
use types::GlyphId;

#[derive(Default)]
struct Steps {
    calls: Vec<u64>,
    stop: Option<usize>,
}
impl WorkControl for Steps {
    type Error = usize;
    fn checkpoint(&mut self, work: u64) -> Result<(), usize> {
        self.calls.push(work);
        if self.stop == Some(self.calls.len()) {
            Err(self.calls.len())
        } else {
            Ok(())
        }
    }
}

#[test]
fn all_32_paint_formats_keep_values_and_transport_each_control_failure() {
    for format in 1..=32u8 {
        let mut bytes = [0u8; 64];
        bytes[0] = format;
        let plain = colr::Paint::read(FontData::new(&bytes)).unwrap();
        let mut control = Steps::default();
        let checked = colr::Paint::try_read(FontData::new(&bytes), &mut control)
            .unwrap()
            .unwrap();
        assert_eq!(plain.format(), checked.format());
        assert_eq!(plain.format(), format);
        assert!(control.calls.len() >= 2);
        for ordinal in 1..=control.calls.len() {
            let mut fail = Steps {
                stop: Some(ordinal),
                ..Default::default()
            };
            assert!(
                matches!(colr::Paint::try_read(FontData::new(&bytes),&mut fail),Err(e) if e==ordinal)
            );
            assert_eq!(fail.calls.len(), ordinal);
        }
    }
    let bytes = [255u8];
    assert!(matches!(
        colr::Paint::try_read(FontData::new(&bytes), &mut Steps::default()),
        Ok(Err(ReadError::InvalidFormat(255)))
    ));
}

#[test]
fn svg_overflow_is_absent_and_control_is_never_absence() {
    let mut bytes = vec![0, 0, 0, 0, 0, 8, 0, 0, 0, 1, 0, 2, 0, 2];
    bytes.extend_from_slice(&u32::MAX.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    let table = svg::Svg::read(FontData::new(&bytes)).unwrap();
    assert_eq!(table.glyph_data(GlyphId::new(2)).unwrap(), None);
    let mut control = Steps::default();
    assert_eq!(
        table
            .try_glyph_data(GlyphId::new(2), &mut control)
            .unwrap()
            .unwrap(),
        None
    );
    assert!(control.calls.iter().any(|work| *work > 0));
    for ordinal in 1..=control.calls.len() {
        let mut fail = Steps {
            stop: Some(ordinal),
            ..Default::default()
        };
        assert!(matches!(table.try_glyph_data(GlyphId::new(2),&mut fail),Err(e) if e==ordinal));
        assert_eq!(fail.calls.len(), ordinal);
    }
}

#[test]
fn bitmap_args_dispatch_remains_font_error_inside_control() {
    for format in 1..=5u16 {
        let mut bytes = [0u8; 32];
        bytes[..2].copy_from_slice(&format.to_be_bytes());
        let args = (types::GlyphId16::new(2), types::GlyphId16::new(0));
        let plain = <bitmap::IndexSubtable as crate::FontReadWithArgs>::read_with_args(
            FontData::new(&bytes),
            &args,
        )
        .unwrap();
        let checked = bitmap::IndexSubtable::try_read_with_args(
            FontData::new(&bytes),
            &args,
            &mut Steps::default(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(plain.index_format(), checked.index_format());
        let mut fail = Steps {
            stop: Some(2),
            ..Default::default()
        };
        assert!(matches!(
            bitmap::IndexSubtable::try_read_with_args(FontData::new(&bytes), &args, &mut fail),
            Err(2)
        ));
    }
}

fn location_table(count: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; 56 + count * 8 + 16];
    bytes[..2].copy_from_slice(&3u16.to_be_bytes());
    bytes[4..8].copy_from_slice(&1u32.to_be_bytes());
    bytes[8..12].copy_from_slice(&56u32.to_be_bytes());
    bytes[12..16].copy_from_slice(&((count * 8 + 16) as u32).to_be_bytes());
    bytes[16..20].copy_from_slice(&(count as u32).to_be_bytes());
    bytes[50..52].copy_from_slice(&2u16.to_be_bytes());
    bytes[54] = 32;
    for index in 0..count {
        let pos = 56 + index * 8;
        if index + 1 == count {
            bytes[pos..pos + 2].copy_from_slice(&2u16.to_be_bytes());
            bytes[pos + 2..pos + 4].copy_from_slice(&2u16.to_be_bytes());
        }
        bytes[pos + 4..pos + 8].copy_from_slice(&((count * 8) as u32).to_be_bytes());
    }
    let pos = 56 + count * 8;
    bytes[pos..pos + 2].copy_from_slice(&1u16.to_be_bytes());
    bytes[pos + 2..pos + 4].copy_from_slice(&17u16.to_be_bytes());
    bytes[pos + 12..pos + 16].copy_from_slice(&10u32.to_be_bytes());
    bytes
}

#[test]
fn location_257_records_charge_skipped_items_before_target_and_parse_before_filter() {
    let mut bytes = location_table(257);
    let table = crate::tables::cblc::Cblc::read(FontData::new(&bytes)).unwrap();
    let size = table.bitmap_sizes()[0];
    let plain = size.location(table.offset_data(), GlyphId::new(2)).unwrap();
    let mut control = Steps::default();
    let checked = size
        .try_location(table.offset_data(), GlyphId::new(2), &mut control)
        .unwrap()
        .unwrap();
    assert_eq!(
        (plain.format, plain.data_offset, plain.data_size),
        (17, 0, 10)
    );
    assert_eq!(
        (checked.format, checked.data_offset, checked.data_size),
        (17, 0, 10)
    );
    let positive: Vec<_> = control
        .calls
        .iter()
        .enumerate()
        .filter(|(_, w)| **w != 0)
        .map(|(i, _)| i + 1)
        .collect();
    assert_eq!(positive.len(), 257);
    let ordinal = positive[256];
    let mut failure = Steps {
        stop: Some(ordinal),
        ..Default::default()
    };
    assert!(
        matches!(size.try_location(table.offset_data(),GlyphId::new(2),&mut failure),Err(e) if e==ordinal)
    );
    assert_eq!(failure.calls.len(), ordinal);
    // The first record does not cover gid2. Its bad subtable is still the original first error.
    bytes[60..64].copy_from_slice(&u32::MAX.to_be_bytes());
    let table = crate::tables::cblc::Cblc::read(FontData::new(&bytes)).unwrap();
    let size = table.bitmap_sizes()[0];
    assert!(matches!(
        size.try_location(table.offset_data(), GlyphId::new(2), &mut Steps::default()),
        Ok(Err(ReadError::OutOfBounds))
    ));
}

#[test]
fn nullable_palette_records_distinguish_null_bad_offset_value_and_control() {
    for offset in [0, 14, u32::MAX] {
        let mut bytes = vec![0, 0, 0, 1, 0, 1, 0, 1];
        bytes.extend_from_slice(&offset.to_be_bytes());
        bytes.extend_from_slice(&[0, 0, 3, 5, 7, 11]);
        let table = crate::tables::cpal::Cpal::read(FontData::new(&bytes)).unwrap();
        let value = table
            .try_color_records_array(&mut Steps::default())
            .unwrap();
        match offset {
            0 => assert!(value.is_none()),
            14 => {
                let colors = value.unwrap().unwrap();
                assert_eq!(colors.len(), 1);
                assert_eq!(
                    (
                        colors[0].blue(),
                        colors[0].green(),
                        colors[0].red(),
                        colors[0].alpha()
                    ),
                    (3, 5, 7, 11)
                );
            }
            _ => assert!(matches!(value, Some(Err(ReadError::OutOfBounds)))),
        }
        let mut failure = Steps {
            stop: Some(1),
            ..Default::default()
        };
        assert!(matches!(
            table.try_color_records_array(&mut failure),
            Err(1)
        ));
    }
}

#[test]
fn enormous_counts_never_overflow_generated_representation_ranges() {
    let mut cblc = [0u8; 8];
    cblc[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let table = crate::tables::cblc::Cblc::read(FontData::new(&cblc)).unwrap();
    assert!(table.bitmap_sizes().is_empty());
    let mut sbix = [0u8; 8];
    sbix[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let table = crate::tables::sbix::Sbix::try_read_with_args(
        FontData::new(&sbix),
        &1,
        &mut Steps::default(),
    )
    .unwrap()
    .unwrap();
    assert!(table.strikes().is_empty());
    let count = u32::MAX.to_be_bytes();
    let list = colr::BaseGlyphList::read(FontData::new(&count)).unwrap();
    assert!(list.base_glyph_paint_records().is_empty());
}
