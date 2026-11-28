// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::{control::test_support::*, outline::OutlineGlyphCollection, request};
use markdown_svg_budget::{Limits, RequestControl, ResourcesRef};
use read_fonts::{control::TryFontRead, TableProvider};

#[test]
fn nullable_store_is_a_font_result_for_ordinary_and_request_draw() {
    let original = font_test_data::varc::CJK_6868;
    let font = FontRef::new(original).unwrap();
    let varc = font.varc().unwrap();
    let table_offset = (varc.offset_data().as_bytes().as_ptr() as usize)
        .checked_sub(original.as_ptr() as usize)
        .unwrap();
    let range = varc.multi_var_store_offset_byte_range();
    assert_eq!(range, 8..12);
    let mut bytes = original.to_vec();
    bytes[table_offset + range.start..table_offset + range.end].fill(0);
    let font = FontRef::new(&bytes).unwrap();
    let gid = font.cmap().unwrap().map_codepoint(0x6868u32).unwrap();
    let plain = OutlineGlyphCollection::new(&font);
    assert_eq!(plain.format(), Some(super::super::OutlineGlyphFormat::Varc));
    let glyph = plain.get(gid).unwrap();
    let mut a = Pen::default();
    let ma = glyph.draw(Size::unscaled(), &mut a);
    let (r, _) = resources(None, Limits::default());
    let checked = request::Outlines::try_new(&font, &r).unwrap();
    assert_eq!(
        checked.format(),
        Some(super::super::OutlineGlyphFormat::Varc)
    );
    let glyph = checked.try_get(gid).unwrap().unwrap();
    let mut b = Pen::default();
    let mb = glyph.draw(Size::unscaled(), &mut b).unwrap();
    match (ma, mb) {
        (Ok(a), Ok(b)) => assert_eq!(metrics(a), metrics(b)),
        (Err(a), Err(b)) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
        _ => panic!("same absent-store font must have same result"),
    }
    assert_eq!(a.0, b.0);
    assert_eq!(r.failure(), None);
    r.validate_retired().unwrap();
}

fn leaf(value: bool) -> Vec<u8> {
    let range = if value { 0i16 } else { 1i16 };
    [
        1u16.to_be_bytes().as_slice(),
        0u16.to_be_bytes().as_slice(),
        range.to_be_bytes().as_slice(),
        range.to_be_bytes().as_slice(),
    ]
    .concat()
}
fn condition_list(and: bool, first: bool) -> Vec<u8> {
    let mut bytes = vec![0, if and { 3 } else { 4 }, 2, 0, 0, 9, 0xFF, 0xFF, 0xFF];
    bytes.extend(leaf(first));
    bytes
}
fn eval(
    bytes: &[u8],
    r: &ResourcesRef,
) -> Result<Result<bool, DrawError>, markdown_svg_budget::Error> {
    let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
    let table = font.varc().unwrap();
    let store = table.multi_var_store().unwrap().unwrap();
    let regions = store.region_list().unwrap();
    let mut cx = RequestControl(r);
    let condition = Condition::try_read(bytes.into(), &mut cx)?.unwrap();
    let mut cache = ScalarCache::<RequestControl<'_>>::try_new(0, &mut cx)?;
    let mut scratch = Scratchpad::<RequestControl<'_>>::try_new(&mut cx)?;
    // These other owners are inline only. The frame allocation naturally
    // retires inside the exact helper before its returned boolean is read.
    control::split(Outlines::try_eval_condition(
        &condition,
        &[],
        &store,
        &regions,
        &mut cache,
        &mut scratch,
        &mut cx,
    ))
}
fn deep_condition(depth: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..depth {
        bytes.extend([0, 5, 0, 0, 5]);
    }
    bytes.extend(leaf(true));
    bytes
}

#[test]
fn condition_frames_preserve_empty_short_circuit_and_nested_not() {
    let cases = [
        (vec![0, 3, 0], true),
        (vec![0, 4, 0], false),
        (condition_list(true, false), false),
        (condition_list(false, true), true),
        (deep_condition(513), false),
    ];
    for (bytes, expected) in cases {
        let (r, _) = resources(None, Limits::default());
        assert_eq!(eval(&bytes, &r).unwrap().unwrap(), expected);
        r.validate_retired().unwrap();
    }
    // The invalid second offset must still produce its original font error
    // when it is actually reached, rather than a control error/default value.
    for bytes in [condition_list(true, true), condition_list(false, false)] {
        let (r, _) = resources(None, Limits::default());
        assert!(eval(&bytes, &r).unwrap().is_err());
        assert_eq!(r.failure(), None);
        r.validate_retired().unwrap();
    }
}

#[test]
fn deep_frame_owner_failures_and_last_natural_refund_reject_the_boolean() {
    let bytes = deep_condition(513);
    let (r, events) = resources(None, Limits::default());
    assert!(!eval(&bytes, &r).unwrap().unwrap());
    r.validate_retired().unwrap();
    let events = events.lock().unwrap();
    let counts = (
        events.reserve,
        events.commit,
        events.release,
        events.positive_work,
    );
    drop(events);
    assert!(counts.0 > 1 && counts.1 > 1 && counts.2 > 1 && counts.3 > 256);
    let mut faults = Vec::new();
    for n in 1..=counts.0 {
        faults.push(Fault::Reserve(n));
    }
    for n in 1..=counts.1 {
        faults.push(Fault::Commit(n));
    }
    for n in 1..=counts.2 {
        faults.push(Fault::Release(n));
    }
    for n in [1, 256, counts.3] {
        faults.push(Fault::Work(n));
    }
    for fault in faults {
        let (r, events) = resources(Some(fault), Limits::default());
        assert_eq!(eval(&bytes, &r).err(), Some(FAULT), "{fault:?}");
        assert_eq!(r.failure(), Some(FAULT));
        let events = events.lock().unwrap();
        assert_eq!(events.injected, 1);
        let failed_refund = u64::from(matches!(fault, Fault::Release(_)));
        assert_eq!(r.statistics().live_leases, failed_refund);
        assert_eq!(r.statistics().release_failures, failed_refund);
        assert_eq!(
            r.statistics().scratch_bytes,
            events.failed_release_bytes as u64
        );
        if matches!(fault, Fault::Release(_)) {
            assert_eq!(r.validate_retired(), Err(FAULT));
        } else {
            r.validate_retired().unwrap();
        }
    }
    println!("condition frames513 reserves/commits/releases/positive_work={counts:?}");
}
