// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::{
    control::test_support::*,
    instance::{LocationRef, NormalizedCoord},
    request::GlyphMetrics,
    MetadataProvider,
};
use markdown_svg_budget::Limits;
use read_fonts::{types::Tag, TableProvider};

const SBIX_PNG: &[u8] = include_bytes!(
    "../../../../../tests/fixtures/svg-font-representations-v1/sbix-png-v1.ttf"
);
const CBDT_PNG: &[u8] = include_bytes!(
    "../../../../../tests/fixtures/svg-font-representations-v1/cbdt-png-v1.ttf"
);
const PNG: &[u8] = include_bytes!(
    "../../../../../tests/fixtures/svg-font-representations-v1/original-2x2-rgba.png"
);

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    floats: [u32; 6],
    advance: Option<u32>,
    dimensions: [u32; 2],
    origin: Origin,
    // These are test observations, never candidate font buffers.
    kind: u8,
    mask: Option<(u8, bool)>,
    bytes: Vec<u8>,
}

fn snapshot(metadata: BitmapMetadata, data: BitmapDataRef<'_>) -> Snapshot {
    let (kind, mask, bytes) = match data {
        BitmapDataRef::Png(bytes) => (0, None, bytes),
        BitmapDataRef::Bgra(bytes) => (1, None, bytes),
        BitmapDataRef::Mask {
            bpp,
            is_packed,
            data,
        } => (2, Some((bpp, is_packed)), data),
    };
    Snapshot {
        floats: [
            metadata.bearing_x,
            metadata.bearing_y,
            metadata.inner_bearing_x,
            metadata.inner_bearing_y,
            metadata.ppem_x,
            metadata.ppem_y,
        ]
        .map(f32::to_bits),
        advance: metadata.advance.map(f32::to_bits),
        dimensions: [metadata.width, metadata.height],
        origin: metadata.placement_origin,
        kind,
        mask,
        bytes: bytes.to_vec(),
    }
}

fn ordinary_snapshot(glyph: bitmap::BitmapGlyph<'_>) -> Snapshot {
    let data = match glyph.data {
        bitmap::BitmapData::Png(bytes) => BitmapDataRef::Png(bytes),
        bitmap::BitmapData::Bgra(bytes) => BitmapDataRef::Bgra(bytes),
        bitmap::BitmapData::Mask(mask) => BitmapDataRef::Mask {
            bpp: mask.bpp,
            is_packed: mask.is_packed,
            data: mask.data,
        },
    };
    snapshot(
        BitmapMetadata {
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
            inner_bearing_x: glyph.inner_bearing_x,
            inner_bearing_y: glyph.inner_bearing_y,
            ppem_x: glyph.ppem_x,
            ppem_y: glyph.ppem_y,
            advance: glyph.advance,
            width: glyph.width,
            height: glyph.height,
            placement_origin: glyph.placement_origin,
        },
        data,
    )
}

fn tables(bytes: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let font = FontRef::new(bytes).unwrap();
    font.table_directory()
        .table_records()
        .iter()
        .map(|record| {
            let tag = record.tag();
            (
                tag.to_be_bytes(),
                font.table_data(tag).unwrap().as_bytes().to_vec(),
            )
        })
        .collect()
}

fn replace_table(bytes: &[u8], tag: [u8; 4], replacement: Vec<u8>) -> Vec<u8> {
    let mut entries = tables(bytes);
    entries.retain(|(old, _)| *old != tag);
    entries.push((tag, replacement));
    sfnt(entries)
}

#[test]
fn original_bitmap_fonts_preserve_all_bytes_metadata_and_selection() {
    for bytes in [
        font_test_data::CBDT,
        font_test_data::NOTO_HANDWRITING_SBIX,
        font_test_data::EMBEDDED_BITMAPS,
        SBIX_PNG,
        CBDT_PNG,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let ordinary = font.bitmap_strikes();
        let (r, _) = resources(None, Limits::default());
        let checked = BitmapStrikes::try_new(&font, &r).unwrap();
        assert_eq!(ordinary.format(), checked.format());
        let mut present = 0;
        for size in [
            Size::unscaled(),
            Size::new(0.0),
            Size::new(2.0),
            Size::new(12.0),
            Size::new(16.0),
            Size::new(17.0),
            Size::new(60.0),
            Size::new(64.0),
            Size::new(109.0),
            Size::new(128.0),
            Size::new(f32::MAX),
        ] {
            for gid in 0..12 {
                let gid = GlyphId::new(gid);
                let expected = ordinary.glyph_for_size(size, gid).map(ordinary_snapshot);
                let actual = checked
                    .try_glyph_for_size(size, gid)
                    .unwrap()
                    .map(|glyph| snapshot(glyph.metadata(), glyph.data()));
                present += usize::from(expected.is_some());
                assert_eq!(actual, expected, "size={size:?} gid={gid:?}");
            }
        }
        assert!(present > 0);
        assert_eq!(r.statistics().live_leases, 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn original_complete_two_by_two_png_is_borrowed_without_decoding() {
    for bytes in [SBIX_PNG, CBDT_PNG] {
        let font = FontRef::new(bytes).unwrap();
        let (r, events) = resources(None, Limits::default());
        let checked = BitmapStrikes::try_new(&font, &r).unwrap();
        let glyph = checked
            .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
            .unwrap()
            .unwrap();
        assert_eq!(glyph.data(), BitmapDataRef::Png(PNG));
        assert_eq!(glyph.metadata().width, 2);
        assert_eq!(glyph.metadata().height, 2);
        assert_eq!(glyph.metadata().ppem_y.to_bits(), 2.0f32.to_bits());
        let BitmapDataRef::Png(data) = glyph.data() else {
            unreachable!()
        };
        assert!(data.as_ptr() >= bytes.as_ptr());
        assert!(data.as_ptr().wrapping_add(data.len()) <= bytes.as_ptr().wrapping_add(bytes.len()));
        assert_eq!(events.lock().unwrap().reserve, 0);
        r.validate_retired().unwrap();
    }
}

fn long_skipped_sbix() -> Vec<u8> {
    let font = FontRef::new(SBIX_PNG).unwrap();
    let original = font.table_data(Tag::new(b"sbix")).unwrap();
    let original = original.as_bytes();
    let original_offset = u32::from_be_bytes(original[8..12].try_into().unwrap()) as usize;
    let count = 257u32;
    let mut table = original[..4].to_vec();
    table.extend(count.to_be_bytes());
    for index in 0..count {
        table.extend(
            if index == count - 1 {
                8 + count * 4
            } else {
                u32::MAX
            }
            .to_be_bytes(),
        );
    }
    table.extend_from_slice(&original[original_offset..]);
    replace_table(SBIX_PNG, *b"sbix", table)
}

#[test]
fn skipped_strike257_is_reached_and_every_work_fault_stays_typed() {
    let bytes = long_skipped_sbix();
    let font = FontRef::new(&bytes).unwrap();
    let ordinary = font.bitmap_strikes();
    assert_eq!(ordinary.len(), 257);
    assert_eq!(ordinary.iter().count(), 1);
    let expected = ordinary
        .glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap();
    let expected = ordinary_snapshot(expected);
    let (r, events) = resources(None, Limits::default());
    let checked = BitmapStrikes::try_new(&font, &r).unwrap();
    let glyph = checked
        .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot(glyph.metadata(), glyph.data()), expected);
    let count = events.lock().unwrap().positive_work;
    assert!(count > 257);
    r.validate_retired().unwrap();
    for ordinal in 1..=count {
        let (r, events) = resources(Some(Fault::Work(ordinal)), Limits::default());
        let result = BitmapStrikes::try_new(&font, &r)
            .and_then(|checked| checked.try_glyph_for_size(Size::unscaled(), GlyphId::new(2)));
        assert_eq!(result.err(), Some(FAULT), "work ordinal={ordinal}");
        let observed = events.lock().unwrap();
        assert_eq!(observed.injected, 1, "work ordinal={ordinal}");
        assert_eq!(observed.positive_work, ordinal);
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(r.statistics().live_leases, 0);
        r.validate_retired().unwrap();
    }
    println!("long skipped sbix work ordinals={count}; 257 attempted strikes, 256 malformed skips, 1 real PNG");
}

fn hvar_many_regions() -> Vec<u8> {
    let count = 257u16;
    let region_bytes = 4 + count as usize * 6;
    let mut store = Vec::new();
    store.extend(1u16.to_be_bytes());
    store.extend(12u32.to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend((12u32 + region_bytes as u32).to_be_bytes());
    store.extend(1u16.to_be_bytes());
    store.extend(count.to_be_bytes());
    for _ in 0..count {
        for coord in [0i16, 16384, 16384] {
            store.extend(coord.to_be_bytes());
        }
    }
    for n in [3u16, count, count] {
        store.extend(n.to_be_bytes());
    }
    for index in 0..count {
        store.extend(index.to_be_bytes());
    }
    for gid in 0..3 {
        for _ in 0..count {
            store.extend((i16::from(gid == 2)).to_be_bytes());
        }
    }
    let map_offset = 20 + store.len() as u32;
    let mut hvar = Vec::new();
    for n in [1u16, 0] {
        hvar.extend(n.to_be_bytes());
    }
    for n in [20u32, 0, map_offset, 0] {
        hvar.extend(n.to_be_bytes());
    }
    hvar.extend(store);
    hvar.extend([0, 1, 0, 3, 0, 1, 2]);
    replace_table(SBIX_PNG, *b"HVAR", hvar)
}

#[test]
fn lsb_hvar257_regions_keeps_original_fixed_math_and_work_failures() {
    let bytes = hvar_many_regions();
    let font = FontRef::new(&bytes).unwrap();
    for coord in [0.0, 0.5, 1.0] {
        let coords = [NormalizedCoord::from_f32(coord)];
        let location = LocationRef::new(&coords);
        let ordinary = font
            .glyph_metrics(Size::unscaled(), location)
            .left_side_bearing(GlyphId::new(2));
        assert_eq!(ordinary, Some((257.0 * coord).round()));
        let (r, events) = resources(None, Limits::default());
        let checked = GlyphMetrics::try_new(&font, Size::unscaled(), location, &r).unwrap();
        assert_eq!(
            checked
                .try_left_side_bearing(GlyphId::new(2))
                .unwrap()
                .map(f32::to_bits),
            ordinary.map(f32::to_bits)
        );
        let count = events.lock().unwrap().positive_work;
        assert!(count > if coord == 0.0 { 0 } else { 257 });
        r.validate_retired().unwrap();
        for ordinal in 1..=count {
            let (r, events) = resources(Some(Fault::Work(ordinal)), Limits::default());
            let result = GlyphMetrics::try_new(&font, Size::unscaled(), location, &r)
                .and_then(|metrics| metrics.try_left_side_bearing(GlyphId::new(2)));
            assert_eq!(result.err(), Some(FAULT), "coord={coord} ordinal={ordinal}");
            let observed = events.lock().unwrap();
            assert_eq!(observed.injected, 1);
            assert_eq!(observed.positive_work, ordinal);
            assert_eq!(r.failure(), Some(FAULT));
            assert_eq!(r.statistics().live_leases, 0);
            r.validate_retired().unwrap();
        }
        println!("HVAR lsb coord={coord} value={ordinary:?} all work ordinals={count}");
    }
}

fn bitmap_with_large_gvar_outline() -> Vec<u8> {
    let font = FontRef::new(SBIX_PNG).unwrap();
    let mut head = font.head().unwrap().offset_data().as_bytes().to_vec();
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    // The original inline outline buffer is 16KiB. 1025 actual points
    // force the real bounds path to acquire and naturally release scratch.
    let mut glyf = Vec::new();
    for n in [1i16, 0, 0, 0, 0] {
        glyf.extend(n.to_be_bytes());
    }
    glyf.extend(1024u16.to_be_bytes());
    glyf.extend(0u16.to_be_bytes());
    for _ in 0..4 {
        glyf.extend([0x39, 255]);
    }
    glyf.push(0x31);
    let mut loca = Vec::new();
    for n in [0u32, 0, 0, glyf.len() as u32] {
        loca.extend(n.to_be_bytes());
    }
    let mut gvar = Vec::new();
    for n in [1u16, 0, 1, 0] {
        gvar.extend(n.to_be_bytes());
    }
    gvar.extend(36u32.to_be_bytes());
    for n in [3u16, 1] {
        gvar.extend(n.to_be_bytes());
    }
    gvar.extend(36u32.to_be_bytes());
    for _ in 0..4 {
        gvar.extend(0u32.to_be_bytes());
    }
    let mut entries = tables(SBIX_PNG);
    entries.retain(|(tag, _)| ![b"head", b"loca", b"glyf", b"gvar"].contains(&tag));
    entries.extend([
        (*b"head", head),
        (*b"loca", loca),
        (*b"glyf", glyf),
        (*b"gvar", gvar),
    ]);
    sfnt(entries)
}

fn default_gvar_with257_components() -> Vec<u8> {
    let mut glyf = Vec::new();
    for n in [-1i16, 0, 0, 0, 0] {
        glyf.extend(n.to_be_bytes());
    }
    for component in 0..257 {
        let flags = if component == 256 { 3u16 } else { 0x23 };
        glyf.extend(flags.to_be_bytes());
        // All components reference the empty .notdef, with XY word args.
        for n in [0u16, 0, 0] {
            glyf.extend(n.to_be_bytes());
        }
    }
    let mut head = FontRef::new(SBIX_PNG)
        .unwrap()
        .head()
        .unwrap()
        .offset_data()
        .as_bytes()
        .to_vec();
    head[50..52].copy_from_slice(&1u16.to_be_bytes());
    let mut loca = Vec::new();
    for n in [0u32, 0, 0, glyf.len() as u32] {
        loca.extend(n.to_be_bytes());
    }
    let mut gvar = Vec::new();
    for n in [1u16, 0, 1, 0] {
        gvar.extend(n.to_be_bytes());
    }
    gvar.extend(36u32.to_be_bytes());
    for n in [3u16, 1] {
        gvar.extend(n.to_be_bytes());
    }
    gvar.extend(36u32.to_be_bytes());
    for _ in 0..4 {
        gvar.extend(0u32.to_be_bytes());
    }
    let mut entries = tables(SBIX_PNG);
    entries.retain(|(tag, _)| ![b"head", b"loca", b"glyf", b"gvar"].contains(&tag));
    entries.extend([
        (*b"head", head),
        (*b"loca", loca),
        (*b"glyf", glyf),
        (*b"gvar", gvar),
    ]);
    sfnt(entries)
}

#[test]
fn default_coords_lsb_gvar_walks_all257_real_components_before_no_tuple() {
    let bytes = default_gvar_with257_components();
    let font = FontRef::new(&bytes).unwrap();
    assert!(font.gvar().is_ok());
    assert!(font.hvar().is_err());
    let ordinary = font
        .glyph_metrics(Size::unscaled(), LocationRef::default())
        .left_side_bearing(GlyphId::new(2));
    assert_eq!(ordinary, Some(0.0));
    let (r, events) = resources(None, Limits::default());
    let checked =
        GlyphMetrics::try_new(&font, Size::unscaled(), LocationRef::default(), &r).unwrap();
    assert_eq!(
        checked.try_left_side_bearing(GlyphId::new(2)).unwrap(),
        ordinary
    );
    let count = events.lock().unwrap().positive_work;
    assert!(count > 257);
    r.validate_retired().unwrap();
    for ordinal in 1..=count {
        let (r, events) = resources(Some(Fault::Work(ordinal)), Limits::default());
        let result = GlyphMetrics::try_new(&font, Size::unscaled(), LocationRef::default(), &r)
            .and_then(|metrics| metrics.try_left_side_bearing(GlyphId::new(2)));
        assert_eq!(
            result.err(),
            Some(FAULT),
            "gvar default work ordinal={ordinal}"
        );
        let observed = events.lock().unwrap();
        assert_eq!(observed.injected, 1);
        assert_eq!(observed.positive_work, ordinal);
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(r.statistics().live_leases, 0);
        r.validate_retired().unwrap();
    }
    println!("gvar default coords with257 real components all work ordinals={count}");
}

#[test]
fn sbix_real_large_gvar_bounds_reserve_commit_and_final_refund_faults_are_outer() {
    let bytes = bitmap_with_large_gvar_outline();
    let font = FontRef::new(&bytes).unwrap();
    let expected = font
        .bitmap_strikes()
        .glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap();
    let (r, events) = resources(None, Limits::default());
    let checked = BitmapStrikes::try_new(&font, &r).unwrap();
    let actual = checked
        .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        snapshot(actual.metadata(), actual.data()),
        ordinary_snapshot(expected)
    );
    let observed = events.lock().unwrap();
    let (reserves, commits, releases) = (observed.reserve, observed.commit, observed.release);
    assert!(reserves > 0 && commits > 0 && releases > 0);
    drop(observed);
    r.validate_retired().unwrap();
    for fault in (1..=reserves)
        .map(Fault::Reserve)
        .chain((1..=commits).map(Fault::Commit))
        .chain((1..=releases).map(Fault::Release))
    {
        let (r, events) = resources(Some(fault), Limits::default());
        let result = BitmapStrikes::try_new(&font, &r)
            .and_then(|checked| checked.try_glyph_for_size(Size::unscaled(), GlyphId::new(2)));
        assert_eq!(result.err(), Some(FAULT), "fault={fault:?}");
        assert_eq!(r.failure(), Some(FAULT));
        let observed = events.lock().unwrap();
        assert_eq!(observed.injected, 1, "fault={fault:?}");
        if matches!(fault, Fault::Release(_)) {
            assert_eq!(
                r.statistics().scratch_bytes,
                observed.failed_release_bytes as u64
            );
            assert_eq!(r.validate_retired(), Err(FAULT));
        } else {
            assert_eq!(r.statistics().live_leases, 0);
            r.validate_retired().unwrap();
        }
    }
    println!("sbix large gvar bbox real owner ordinals reserve={reserves} commit={commits} release={releases}; final refund failure retained");
}

#[test]
fn failed_collection_retains_its_original_account() {
    let bytes = long_skipped_sbix();
    let font = FontRef::new(&bytes).unwrap();
    let (source, events) = resources(None, Limits::default());
    let checked = BitmapStrikes::try_new(&font, &source).unwrap();
    source.record(FAULT);
    let (other, _) = resources(None, Limits::default());
    let other_collection = BitmapStrikes::try_new(&font, &other).unwrap();
    assert!(other_collection
        .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap()
        .is_some());
    let before = events.lock().unwrap().work.len();
    assert_eq!(
        checked
            .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
            .err(),
        Some(FAULT)
    );
    assert_eq!(events.lock().unwrap().work.len(), before);
    assert_eq!(other.failure(), None);
    other.validate_retired().unwrap();
}

#[test]
fn original_missing_bad_table_and_non_png_sbix_recovery_is_unchanged() {
    for graphic in [*b"jpg ", *b"tiff", *b"dupe"] {
        let font = FontRef::new(SBIX_PNG).unwrap();
        let mut sbix = font
            .table_data(Tag::new(b"sbix"))
            .unwrap()
            .as_bytes()
            .to_vec();
        let strike = u32::from_be_bytes(sbix[8..12].try_into().unwrap()) as usize;
        let glyph = u32::from_be_bytes(sbix[strike + 12..strike + 16].try_into().unwrap()) as usize;
        sbix[strike + glyph + 4..strike + glyph + 8].copy_from_slice(&graphic);
        let bytes = replace_table(SBIX_PNG, *b"sbix", sbix);
        let font = FontRef::new(&bytes).unwrap();
        assert!(font
            .bitmap_strikes()
            .glyph_for_size(Size::unscaled(), GlyphId::new(2))
            .is_none());
        let (r, _) = resources(None, Limits::default());
        let checked = BitmapStrikes::try_new(&font, &r).unwrap();
        assert!(checked
            .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
            .unwrap()
            .is_none());
        r.validate_retired().unwrap();
    }
    let bytes = replace_table(CBDT_PNG, *b"sbix", vec![0, 1]);
    let font = FontRef::new(&bytes).unwrap();
    let (r, _) = resources(None, Limits::default());
    let checked = BitmapStrikes::try_new(&font, &r).unwrap();
    assert_eq!(checked.format(), Some(BitmapFormat::Cbdt));
    assert_eq!(font.bitmap_strikes().format(), checked.format());
    let actual = checked
        .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap()
        .unwrap();
    assert_eq!(actual.data(), BitmapDataRef::Png(PNG));
    r.validate_retired().unwrap();
    let empty = sfnt(vec![(*b"name", vec![])]);
    let font = FontRef::new(&empty).unwrap();
    let checked = BitmapStrikes::try_new(&font, &r).unwrap();
    assert_eq!(checked.format(), None);
    assert!(checked
        .try_glyph_for_size(Size::unscaled(), GlyphId::new(2))
        .unwrap()
        .is_none());
    r.validate_retired().unwrap();
}

#[test]
fn bitmap_operation_error_precedes_its_real_bounds_refund_error() {
    use markdown_svg_budget::{
        Backend, Error, FirstFailureRef, ResourceKind, ResourcesRef, Ticket,
    };
    use std::sync::{Arc, Mutex};
    const OPERATION: Error = Error::native(-0x5E02);
    const REFUND: Error = Error::native(-0x5E03);
    #[derive(Default)]
    struct Observed {
        live: usize,
        reserved: u32,
        operation: usize,
        refunds: usize,
    }
    struct Host(Arc<Mutex<Observed>>);
    impl Backend for Host {
        fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, Error> {
            let mut observed = self.0.lock().unwrap();
            observed.reserved += 1;
            observed.live += 1;
            Ticket::new(0x5E02, observed.reserved, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, _: Ticket) -> Result<(), Error> {
            let mut observed = self.0.lock().unwrap();
            observed.live -= 1;
            if observed.operation != 0 {
                observed.refunds += 1;
                Err(REFUND)
            } else {
                Ok(())
            }
        }
        fn checkpoint(&self, work: u64) -> Result<(), Error> {
            let mut observed = self.0.lock().unwrap();
            if work != 0 && observed.live != 0 {
                observed.operation += 1;
                Err(OPERATION)
            } else {
                Ok(())
            }
        }
    }
    let observed = Arc::new(Mutex::new(Observed::default()));
    let r = ResourcesRef::new_prepaid(
        Host(observed.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    let bytes = bitmap_with_large_gvar_outline();
    let font = FontRef::new(&bytes).unwrap();
    let result = BitmapStrikes::try_new(&font, &r)
        .and_then(|checked| checked.try_glyph_for_size(Size::unscaled(), GlyphId::new(2)));
    assert_eq!(result.err(), Some(OPERATION));
    assert_eq!(r.failure(), Some(OPERATION));
    let observed = observed.lock().unwrap();
    assert_eq!(observed.operation, 1);
    assert!(observed.refunds > 0);
    assert_eq!(observed.live, 0);
    assert!(r.statistics().scratch_bytes > 0);
    assert_eq!(r.validate_retired(), Err(OPERATION));
    println!("sbix actual bounds operation fault before {} real refund faults; first operation preserved", observed.refunds);
}
