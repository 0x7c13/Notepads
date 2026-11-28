// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::{
    control::test_support::*,
    instance::{LocationRef, Size},
    MetadataProvider,
};
use markdown_svg_budget::{FailureKind, LimitKind, Limits};
use read_fonts::{
    types::{F2Dot14, Tag},
    TableProvider,
};

/// A valid one-contour/1025-point font made from the fixed upstream simple
/// font's scalar metadata. This is a new small fixture, not a new expected
/// upstream baseline. Repeated coordinate flags cover all points without an
/// enormous source fixture; its actual glyph scratch is >the16KiB stack path.
fn large_static_font() -> Vec<u8> {
    let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
    let mut tables = Vec::new();
    for tag in [*b"head", *b"hhea", *b"hmtx", *b"maxp"] {
        let mut data = font
            .data_for_tag(Tag::new(&tag))
            .unwrap()
            .as_bytes()
            .to_vec();
        match &tag {
            b"head" => data[50..52].copy_from_slice(&1i16.to_be_bytes()),
            b"hhea" => data[34..36].copy_from_slice(&1u16.to_be_bytes()),
            b"hmtx" => data.truncate(4),
            b"maxp" => {
                data[4..6].copy_from_slice(&1u16.to_be_bytes());
                if data.len() >= 10 {
                    data[6..8].copy_from_slice(&1025u16.to_be_bytes());
                    data[8..10].copy_from_slice(&1u16.to_be_bytes());
                }
            }
            _ => unreachable!(),
        }
        tables.push((tag, data));
    }
    let mut glyf = Vec::new();
    glyf.extend(1i16.to_be_bytes());
    glyf.extend([0u8; 8]);
    glyf.extend(1024u16.to_be_bytes());
    glyf.extend(0u16.to_be_bytes());
    for _ in 0..4 {
        glyf.extend([0x39, 255]);
    }
    glyf.push(0x31);
    let length = glyf.len() as u32;
    tables.push((*b"glyf", glyf));
    tables.push((
        *b"loca",
        [0u32.to_be_bytes(), length.to_be_bytes()].concat(),
    ));
    sfnt(tables)
}
fn draw_large(resources: &ResourcesRef) -> Result<Result<AdjustedMetrics, DrawError>, Error> {
    let data = large_static_font();
    let font = FontRef::try_new(&data, &mut RequestControl(resources))?.unwrap();
    let outlines = Outlines::try_new(&font, resources)?;
    let glyph = outlines.try_get(GlyphId::new(0))?.unwrap();
    let mut pen = Pen::default();
    let result = glyph.draw(Size::unscaled(), &mut pen)?;
    resources.checkpoint(0)?;
    Ok(result)
}

#[test]
fn real_all_glyph_paths_metrics_and_borrowed_locations_match_ordinary() {
    for data in [
        font_test_data::VAZIRMATN_VAR,
        font_test_data::CANTARELL_VF_TRIMMED,
        font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
        font_test_data::varc::CJK_6868,
    ] {
        let ordinary_font = FontRef::new(data).unwrap();
        let ordinary = ordinary_font.outline_glyphs();
        let glyph_count = ordinary_font.maxp().unwrap().num_glyphs() as u32;
        let (r, _) = resources(None, Limits::default());
        let font = FontRef::try_new(data, &mut RequestControl(&r))
            .unwrap()
            .unwrap();
        let checked = Outlines::try_new(&font, &r).unwrap();
        assert_eq!(ordinary.format(), checked.format());
        let mut compared = 0;
        for (coordinate, path_style) in [0.0, 0.5].into_iter().flat_map(|coordinate| {
            [
                crate::outline::pen::PathStyle::FreeType,
                crate::outline::pen::PathStyle::HarfBuzz,
            ]
            .into_iter()
            .map(move |style| (coordinate, style))
        }) {
            // A fresh short location borrow each time, independent of font life.
            let coords = [F2Dot14::from_f32(coordinate); 3];
            for id in 0..glyph_count {
                let id = GlyphId::new(id);
                let plain = ordinary.get(id);
                let controlled = checked.try_get(id).unwrap();
                assert_eq!(plain.is_some(), controlled.is_some());
                if let (Some(plain), Some(controlled)) = (plain, controlled) {
                    let mut a = Pen::default();
                    let mut b = Pen::default();
                    let ma = plain.draw(
                        DrawSettings::unhinted(Size::new(17.5), LocationRef::new(&coords))
                            .with_path_style(path_style),
                        &mut a,
                    );
                    let mb = controlled
                        .draw(
                            DrawSettings::unhinted(Size::new(17.5), LocationRef::new(&coords))
                                .with_path_style(path_style),
                            &mut b,
                        )
                        .unwrap();
                    assert_eq!(ma.is_ok(), mb.is_ok(), "glyph {:?}", id);
                    match (ma, mb) {
                        (Ok(ma), Ok(mb)) => assert_eq!(metrics(ma), metrics(mb)),
                        (Err(a), Err(b)) => assert_eq!(format!("{a:?}"), format!("{b:?}")),
                        _ => unreachable!(),
                    }
                    assert_eq!(a.0, b.0, "all event coordinates/close for glyph {:?}", id);
                    compared += 1;
                }
            }
        }
        assert!(compared > 0);
        drop(checked);
        assert_eq!(r.failure(), None);
        assert_eq!(r.statistics().live_leases, 0);
        assert_eq!(r.statistics().scratch_bytes, 0);
        r.validate_retired().unwrap();
        println!(
            "outline format={:?} glyphs={} comparisons={} work={}",
            ordinary.format(),
            glyph_count,
            compared,
            r.statistics().work
        );
    }
}

#[test]
fn real_large_glyph_natural_reserve_commit_retirement_failures_stop_candidate() {
    let (r, events) = resources(None, Limits::default());
    draw_large(&r).unwrap().unwrap();
    let events = events.lock().unwrap();
    let counts = (
        events.reserve,
        events.commit,
        events.release,
        events.positive_work,
    );
    assert!(counts.0 > 0 && counts.1 > 0 && counts.2 > 0 && counts.3 > 256);
    assert!(events.sizes.iter().any(|n| *n > 16384));
    drop(events);
    assert_eq!(r.statistics().live_leases, 0);
    r.validate_retired().unwrap();
    let mut faults = Vec::new();
    for index in 1..=counts.0 {
        faults.push(Fault::Reserve(index));
    }
    for index in 1..=counts.1 {
        faults.push(Fault::Commit(index));
    }
    for index in 1..=counts.2 {
        faults.push(Fault::Release(index));
    }
    for index in [1, 256, counts.3] {
        faults.push(Fault::Work(index));
    }
    for fault in faults {
        let (r, events) = resources(Some(fault), Limits::default());
        assert_eq!(draw_large(&r).err(), Some(FAULT), "{fault:?}");
        assert_eq!(r.failure(), Some(FAULT));
        let before = events.lock().unwrap().work.len();
        let data = large_static_font();
        let font = FontRef::new(&data).unwrap();
        assert_eq!(Outlines::try_new(&font, &r).err(), Some(FAULT));
        let events = events.lock().unwrap();
        assert_eq!(events.injected, 1);
        assert_eq!(events.work.len(), before);
        let failed_refund = u64::from(matches!(fault, Fault::Release(_)));
        assert_eq!(r.statistics().live_leases, failed_refund);
        assert_eq!(
            r.statistics().scratch_bytes,
            events.failed_release_bytes as u64
        );
        assert_eq!(
            r.statistics().release_failures,
            u64::from(matches!(fault, Fault::Release(_)))
        );
        if matches!(fault, Fault::Release(_)) {
            assert_eq!(r.validate_retired(), Err(FAULT));
        } else {
            r.validate_retired().unwrap();
        }
    }
    println!("large glyph real reserves/commits/releases/positive_work={counts:?}");
}

#[test]
fn callback_first_failure_blocks_every_later_pen_event() {
    struct FaultPen<'r> {
        r: &'r ResourcesRef,
        calls: usize,
    }
    impl OutlinePen for FaultPen<'_> {
        fn move_to(&mut self, _: f32, _: f32) {
            self.calls += 1;
            self.r.record(FAULT);
        }
        fn line_to(&mut self, _: f32, _: f32) {
            self.calls += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.calls += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.calls += 1;
        }
        fn close(&mut self) {
            self.calls += 1;
        }
    }
    for data in [
        font_test_data::VAZIRMATN_VAR,
        font_test_data::CANTARELL_VF_TRIMMED,
        font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
    ] {
        let (r, _) = resources(None, Limits::default());
        let font = FontRef::new(data).unwrap();
        let outlines = Outlines::try_new(&font, &r).unwrap();
        let mut pen = FaultPen { r: &r, calls: 0 };
        let glyph = outlines.try_get(GlyphId::new(1)).unwrap().unwrap();
        assert_eq!(glyph.draw(Size::unscaled(), &mut pen).err(), Some(FAULT));
        assert_eq!(pen.calls, 1);
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(outlines.try_get(GlyphId::new(2)).err(), Some(FAULT));
        assert_eq!(r.statistics().live_leases, 0);
    }
}

#[test]
fn empty_collection_and_external_scratch_rejection_preserve_first_error() {
    let font = FontRef::new(font_test_data::NAMES_ONLY).unwrap();
    let (r, _) = resources(None, Limits::default());
    let outlines = Outlines::try_new(&font, &r).unwrap();
    assert_eq!(outlines.format(), None);
    assert!(outlines.try_get(GlyphId::new(0)).unwrap().is_none());
    let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
    let outlines = Outlines::try_new(&font, &r).unwrap();
    let glyph = outlines.try_get(GlyphId::new(0)).unwrap().unwrap();
    let mut external = [0u8; 0];
    let mut pen = Pen::default();
    let error = Error::new(FailureKind::Unsupported, LimitKind::None);
    assert_eq!(
        glyph
            .draw(
                DrawSettings::unhinted(Size::unscaled(), LocationRef::default())
                    .with_memory(Some(&mut external)),
                &mut pen
            )
            .err(),
        Some(error)
    );
    assert!(pen.0.is_empty());
    assert_eq!(r.failure(), Some(error));
    assert_eq!(r.statistics().live_leases, 0);
    // Hinting construction is ordinary fixture setup, outside the request.
    // The checked draw must reject the mode before interpreting any hints.
    let ordinary = font.outline_glyphs();
    let hinting = crate::outline::HintingInstance::new(
        &ordinary,
        Size::new(17.5),
        LocationRef::default(),
        crate::outline::Engine::Interpreter,
    )
    .unwrap();
    let (hint_r, hint_events) = resources(None, Limits::default());
    let checked = Outlines::try_new(&font, &hint_r).unwrap();
    let glyph = checked.try_get(GlyphId::new(0)).unwrap().unwrap();
    assert_eq!(
        glyph
            .draw(DrawSettings::hinted(&hinting, false), &mut pen)
            .err(),
        Some(error)
    );
    assert!(pen.0.is_empty());
    assert_eq!(hint_r.statistics().live_leases, 0);
    assert_eq!(hint_events.lock().unwrap().reserve, 0);
    assert_eq!(hint_r.failure(), Some(error));
    hint_r.validate_retired().unwrap();
    // A fresh existing native fault wins over validation; no replacement slot.
    let (r, _) = resources(None, Limits::default());
    let outlines = Outlines::try_new(&font, &r).unwrap();
    let glyph = outlines.try_get(GlyphId::new(0)).unwrap().unwrap();
    r.record(FAULT);
    assert_eq!(glyph.draw(Size::unscaled(), &mut pen).err(), Some(FAULT));
    assert!(pen.0.is_empty());
}

fn draw_shift_case(
    data: &[u8],
    resources: &ResourcesRef,
    style: crate::outline::pen::PathStyle,
    pen: &mut Pen,
) -> Result<Result<AdjustedMetrics, DrawError>, Error> {
    let font = FontRef::try_new(data, &mut RequestControl(resources))?.unwrap();
    let outlines = Outlines::try_new(&font, resources)?;
    let glyph = outlines.try_get(GlyphId::new(0))?.unwrap();
    glyph.draw(
        DrawSettings::unhinted(Size::unscaled(), LocationRef::default()).with_path_style(style),
        pen,
    )
}

#[test]
fn shifted_large_outline_pays_every_point_and_stops_before_second_chunk() {
    use crate::outline::pen::PathStyle;
    let shifted = large_static_font();
    let font = FontRef::new(&shifted).unwrap();
    let hmtx = font.data_for_tag(Tag::new(b"hmtx")).unwrap();
    assert_eq!(
        i16::from_be_bytes(hmtx.as_bytes()[2..4].try_into().unwrap()),
        100
    );
    let offset = hmtx.as_bytes().as_ptr() as usize - shifted.as_ptr() as usize;
    let mut unshifted = shifted.clone();
    unshifted[offset + 2..offset + 4].copy_from_slice(&0i16.to_be_bytes());
    assert_eq!(
        shifted
            .iter()
            .zip(&unshifted)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
    for style in [PathStyle::FreeType, PathStyle::HarfBuzz] {
        let mut baselines = Vec::new();
        for data in [&unshifted, &shifted] {
            let ordinary_font = FontRef::new(data).unwrap();
            let ordinary = ordinary_font.outline_glyphs();
            let glyph = ordinary.get(GlyphId::new(0)).unwrap();
            let mut expected_pen = Pen::default();
            let expected_metrics = glyph
                .draw(
                    DrawSettings::unhinted(Size::unscaled(), LocationRef::default())
                        .with_path_style(style),
                    &mut expected_pen,
                )
                .unwrap();
            let (r, events) = resources(None, Limits::default());
            let mut actual_pen = Pen::default();
            let actual_metrics = draw_shift_case(data, &r, style, &mut actual_pen)
                .unwrap()
                .unwrap();
            assert_eq!(actual_pen.0, expected_pen.0);
            assert_eq!(metrics(actual_metrics), metrics(expected_metrics));
            assert_eq!(r.failure(), None);
            r.validate_retired().unwrap();
            let work = events
                .lock()
                .unwrap()
                .work
                .iter()
                .copied()
                .filter(|v| *v != 0)
                .collect::<Vec<_>>();
            baselines.push((r.statistics().work, work));
        }
        println!(
            "shift style={style:?} work-zero={} work-shift={}",
            baselines[0].0, baselines[1].0
        );
        // Same original font and point count. Only the nonzero shift performs
        // this additional arithmetic; no new input cap or changed path math.
        assert_eq!(baselines[1].0 - baselines[0].0, 1025);
        let first = baselines[0]
            .1
            .iter()
            .zip(&baselines[1].1)
            .position(|(a, b)| a != b)
            .unwrap();
        assert_eq!(&baselines[1].1[first..first + 5], &[256, 256, 256, 256, 1]);
        assert_eq!(baselines[0].1[first], 1);
        let second_chunk_ordinal = first + 2;
        let (r, events) = resources(Some(Fault::Work(second_chunk_ordinal)), Limits::default());
        let mut pen = Pen::default();
        assert_eq!(
            draw_shift_case(&shifted, &r, style, &mut pen).err(),
            Some(FAULT)
        );
        assert!(pen.0.is_empty());
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(r.statistics().live_leases, 0);
        assert_eq!(r.statistics().scratch_bytes, 0);
        r.validate_retired().unwrap();
        let observed = events.lock().unwrap();
        assert_eq!(observed.injected, 1);
        assert_eq!(observed.positive_work, second_chunk_ordinal);
        assert_eq!(observed.work.last(), Some(&256));
        let before_events = observed.work.len();
        drop(observed);
        let font = FontRef::new(&shifted).unwrap();
        assert_eq!(Outlines::try_new(&font, &r).err(), Some(FAULT));
        assert_eq!(events.lock().unwrap().work.len(), before_events);
        println!("shift style={style:?} second-chunk ordinal={second_chunk_ordinal} stopped-before-pen=true");
    }
}

#[test]
fn scaled_outline_second_chunk_never_mutates_point257_or_later() {
    use crate::outline::glyf::ScaledOutline;
    use read_fonts::{
        tables::glyf::{PointCoord, PointFlags},
        types::{F26Dot6, Point},
    };
    #[derive(Default)]
    struct ShiftWork {
        work: Vec<u64>,
        positive: usize,
        stop_at_second: bool,
    }
    impl markdown_svg_budget::WorkControl for ShiftWork {
        type Error = Error;
        fn checkpoint(&mut self, work: u64) -> Result<(), Self::Error> {
            self.work.push(work);
            if work != 0 {
                self.positive += 1;
                if self.stop_at_second && self.positive == 2 {
                    return Err(FAULT);
                }
            }
            Ok(())
        }
    }
    fn check<T: PointCoord + PartialEq + core::fmt::Debug>(base: T, shift: T, shifted: T, zero: T) {
        let original = Point { x: base, y: base };
        let mut points = vec![original; 1025];
        let mut flags = vec![PointFlags::on_curve(); points.len()];
        let mut contours = [1024];
        let mut phantom = [Point { x: zero, y: zero }; 4];
        phantom[0].x = shift;
        let mut cx = ShiftWork {
            stop_at_second: true,
            ..Default::default()
        };
        assert_eq!(
            ScaledOutline::try_new(
                &mut points,
                phantom,
                &mut flags,
                &mut contours,
                None,
                &mut cx
            )
            .err(),
            Some(FAULT)
        );
        assert_eq!(cx.work, [0, 256, 256]);
        // Exact borrowed-storage evidence, with no getter or outer pen gate:
        // the rejected second window has not subtracted its first point.
        for (index, point) in points.iter().enumerate() {
            assert_eq!(point.x, if index < 256 { shifted } else { base });
            assert_eq!(point.y, base);
        }
        points.fill(original);
        let mut cx = ShiftWork::default();
        let scaled = ScaledOutline::try_new(
            &mut points,
            phantom,
            &mut flags,
            &mut contours,
            None,
            &mut cx,
        )
        .unwrap();
        assert!(scaled.points.iter().all(|p| p.x == shifted && p.y == base));
        drop(scaled);
        assert_eq!(cx.work, [0, 256, 256, 256, 256, 1, 0]);
        points.fill(original);
        phantom[0].x = zero;
        let mut cx = ShiftWork {
            stop_at_second: true,
            ..Default::default()
        };
        let scaled = ScaledOutline::try_new(
            &mut points,
            phantom,
            &mut flags,
            &mut contours,
            None,
            &mut cx,
        )
        .unwrap();
        assert!(scaled.points.iter().all(|p| *p == original));
        drop(scaled);
        assert_eq!(cx.work, [0, 0]);
    }
    check(100f32, 5f32, 95f32, 0f32);
    check(
        F26Dot6::from_i32(100),
        F26Dot6::from_i32(5),
        F26Dot6::from_i32(95),
        F26Dot6::ZERO,
    );
}
