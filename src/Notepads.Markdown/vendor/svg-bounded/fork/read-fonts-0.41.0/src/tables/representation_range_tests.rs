// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Unchanged probes also compiled against the original source mirror on i686.
use crate::{FontData, FontRead};

#[test]
fn cblc_u32_max_range_is_empty() {
    let mut bytes = [0u8; 8];
    bytes[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let table = crate::tables::cblc::Cblc::read(FontData::new(&bytes)).unwrap();
    assert!(table.bitmap_sizes().is_empty());
}
#[test]
fn sbix_u32_max_range_is_empty() {
    let mut bytes = [0u8; 8];
    bytes[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    let table = crate::tables::sbix::Sbix::read(FontData::new(&bytes), 1).unwrap();
    assert!(table.strikes().is_empty());
}
#[test]
fn colr_u32_max_range_is_empty() {
    let bytes = u32::MAX.to_be_bytes();
    let list = crate::tables::colr::BaseGlyphList::read(FontData::new(&bytes)).unwrap();
    assert!(list.base_glyph_paint_records().is_empty());
}
