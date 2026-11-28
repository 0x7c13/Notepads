// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::hb::glyph_array::tests::support::{setup, setup_with_limits};
use crate::{Language, ShaperInstance, UnicodeBuffer};
use std::sync::atomic::Ordering;

const TTC: &[u8] = include_bytes!(
    "../../../../../../tests/fixtures/svg-fonts/Notepads-Fixture-RegularBold.ttc"
);
const ARABIC:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/vazirmatn_var_trimmed.ttf");
const LATIN:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/tinos_subset.ttf");
const AAT:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/TestMORXThirtyfour.ttf");
const AAT_NONCONTEXT:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/TestMORXOne.ttf");
const AAT_REARRANGE:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/TestMORXFour.ttf");
const AAT_CONTEXT:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/TestMORXEighteen.ttf");
const AAT_LIGATURE:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/TestMORXFourtyone.ttf");
const AXES:&[u8]=include_bytes!("../../../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/Amstelvar-avar2.A.ttf");

fn ordinary(
    font: &FontRef<'_>,
    text: &str,
    language: &str,
    direction: Direction,
    vars: &[Variation],
) -> (std::vec::Vec<[u32; 5]>, std::vec::Vec<[u32; 5]>) {
    let data = ShaperData::new(font);
    let instance = ShaperInstance::from_variations(font, vars.iter().copied());
    let shaper = data.shaper(font).instance(Some(&instance)).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_language(Language::new(language).unwrap());
    buffer.set_direction(direction);
    buffer.guess_segment_properties();
    let out = shaper.shape(buffer, ShapeOptions::new());
    if font.data().len() == AAT.len() {
        eprintln!(
            "AAT ordinary len={} successful={}",
            out.len(),
            out.0.successful
        );
    }
    (
        bytemuck::cast_slice(out.glyph_infos()).to_vec(),
        bytemuck::cast_slice(out.glyph_positions()).to_vec(),
    )
}

fn checked(
    r: &ResourcesRef,
    font: &FontRef<'_>,
    text: &str,
    language: &str,
    direction: Direction,
    vars: &[Variation],
) -> Result<(std::vec::Vec<[u32; 5]>, std::vec::Vec<[u32; 5]>), Error> {
    let result = (|| {
        let data = Data::try_new(font, r)?;
        let instance = Instance::try_from_variations(&data, vars.iter().copied())?;
        let mut buffer = Buffer::try_new(r)?;
        buffer.try_push_str(text)?;
        buffer.try_set_language(Some(language))?;
        buffer.try_set_direction(direction)?;
        buffer.try_guess_segment_properties()?;
        let out = try_shape(r, &data, Some(&instance), buffer, ShapeOptions::new(), None)?;
        // Output deliberately outlives both font/cache wrappers.
        drop(instance);
        drop(data);
        let result = (
            bytemuck::cast_slice(out.glyph_infos()).to_vec(),
            bytemuck::cast_slice(out.glyph_positions()).to_vec(),
        );
        drop(out);
        r.checkpoint(0)?;
        Ok(result)
    })();
    let result = result.map_err(|e| r.record(e));
    let value = result?;
    r.checkpoint(0)?;
    Ok(value)
}

#[test]
fn actual_fonts_all_glyph_words_match_for_ttc_rtl_variable_and_four_aat_kinds() {
    let vars = [Variation {
        tag: crate::Tag::new(b"wght"),
        value: 700.0,
    }];
    for (bytes, index, text, language, direction, vars) in [
        (
            TTC,
            0,
            "office A\u{301} ffi",
            "en-US",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            TTC,
            1,
            "אבג אב\u{5B0}",
            "he",
            Direction::RightToLeft,
            &[][..],
        ),
        (
            LATIN,
            0,
            "office ffi AV",
            "en",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            ARABIC,
            0,
            "سلام العربية",
            "ar",
            Direction::RightToLeft,
            &vars[..],
        ),
        (
            AAT_NONCONTEXT,
            0,
            "abcde abcde",
            "en",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            AAT_REARRANGE,
            0,
            "abcde abcde",
            "en",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            AAT_CONTEXT,
            0,
            "abcde abcde",
            "en",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            AAT_LIGATURE,
            0,
            "abcde abcde",
            "en",
            Direction::LeftToRight,
            &[][..],
        ),
        (
            AXES,
            0,
            "AV office",
            "en",
            Direction::LeftToRight,
            &vars[..],
        ),
        (TTC, 0, "각각 कि क्त", "hi", Direction::LeftToRight, &[][..]),
    ] {
        let font = FontRef::from_index(bytes, index).unwrap();
        let expected = ordinary(&font, text, language, direction, vars);
        let (r, state) = setup();
        let actual = checked(&r, &font, text, language, direction, vars).unwrap_or_else(|error| {
            panic!(
                "language={language},index={index},fontBytes={}, error={error:?}, stats={:?}",
                bytes.len(),
                r.statistics()
            )
        });
        assert_eq!(actual, expected, "{language} TTC{index}");
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn expanding_aat_wide_work_still_rejects_the_original_terminating_guard() {
    let font = FontRef::from_index(AAT, 0).unwrap();
    let expected = ordinary(&font, "abcde abcde", "en", Direction::LeftToRight, &[]);
    assert_eq!(expected.0.len(), 65_531);
    let (r, state) = setup_with_limits(markdown_svg_budget::Limits {
        work: u64::MAX,
        ..Default::default()
    });
    let error = checked(&r, &font, "abcde abcde", "en", Direction::LeftToRight, &[]).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Work));
    assert!(r.statistics().work > markdown_svg_budget::Limits::default().work);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    eprintln!(
        "Expanding AAT remains rejected with wide external work: {:?}",
        r.statistics()
    );
}

#[test]
fn expanding_aat_preserves_the_actual_default_work_rejection_and_retires() {
    let font = FontRef::from_index(AAT, 0).unwrap();
    let (r, state) = setup();
    let error = checked(&r, &font, "abcde abcde", "en", Direction::LeftToRight, &[]).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Work));
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    eprintln!(
        "Default expanding AAT remains rejected: {:?}",
        r.statistics()
    );
}

#[test]
fn every_observed_real_reserve_and_commit_fault_keeps_original_error_and_retires() {
    let font = FontRef::from_index(TTC, 0).unwrap();
    let (r, state) = setup();
    checked(
        &r,
        &font,
        "office ffi AV",
        "en",
        Direction::LeftToRight,
        &[],
    )
    .unwrap();
    let reserves = state.reserves.load(Ordering::Relaxed);
    let commits = state.commits.load(Ordering::Relaxed);
    let releases = state.releases.load(Ordering::Relaxed);
    assert!(reserves > 8 && commits > 8);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    for (commit, count) in [(false, reserves), (true, commits)] {
        for ordinal in 1..=count {
            let (r, state) = setup();
            if commit {
                state.fail_commit.store(ordinal, Ordering::Relaxed);
            } else {
                state.fail_reserve.store(ordinal, Ordering::Relaxed);
            }
            let expected = Error::native(if commit { -811 } else { -810 });
            let error = checked(
                &r,
                &font,
                "office ffi AV",
                "en",
                Direction::LeftToRight,
                &[],
            )
            .unwrap_err();
            assert_eq!(error, expected, "commit={commit},ordinal={ordinal}");
            assert_eq!(r.failure(), Some(expected));
            assert_eq!(state.live(), 0);
            r.validate_retired().unwrap();
        }
    }
    for ordinal in 1..=releases {
        let (r, state) = setup();
        state.fail_release.store(ordinal, Ordering::Relaxed);
        let error = checked(
            &r,
            &font,
            "office ffi AV",
            "en",
            Direction::LeftToRight,
            &[],
        )
        .unwrap_err();
        assert_eq!(error, Error::native(-812), "natural refund={ordinal}");
        assert_eq!(r.failure(), Some(error));
        assert!(state.releases.load(Ordering::Relaxed) >= ordinal);
        assert_eq!(
            state.entries.lock().unwrap().len(),
            1,
            "exact failed natural ticket={ordinal}"
        );
        assert_eq!(r.statistics().release_failures, 1);
    }
    eprintln!("real run fault matrix: reserves={reserves}, commits={commits}, naturalRefunds={releases}; every ordinal hit");
}

#[test]
fn output_and_request_font_keys_prevent_cross_request_or_ttc_index_reuse() {
    let font0 = FontRef::from_index(TTC, 0).unwrap();
    let font1 = FontRef::from_index(TTC, 1).unwrap();
    let (r, state) = setup();
    let data0 = Data::try_new(&font0, &r).unwrap();
    let data1 = Data::try_new(&font1, &r).unwrap();
    let instance = Instance::try_from_variations(&data0, [] as [Variation; 0]).unwrap();
    let mut buffer = Buffer::try_new(&r).unwrap();
    buffer.try_push_str("abc").unwrap();
    buffer.try_guess_segment_properties().unwrap();
    assert!(try_shape(
        &r,
        &data1,
        Some(&instance),
        buffer,
        ShapeOptions::new(),
        None
    )
    .is_err());
    drop(instance);
    drop(data0);
    drop(data1);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    let (r1, s1) = setup();
    let (r2, s2) = setup();
    let data = Data::try_new(&font0, &r1).unwrap();
    let buffer = Buffer::try_new(&r2).unwrap();
    assert!(try_shape(&r1, &data, None, buffer, ShapeOptions::new(), None).is_err());
    drop(data);
    assert_eq!(s1.live() + s2.live(), 0);
    r1.validate_retired().unwrap();
    r2.validate_retired().unwrap();
}

#[test]
fn natural_refund_failure_stops_before_a_second_run() {
    let font = FontRef::from_index(TTC, 0).unwrap();
    let (r, state) = setup();
    state.fail_release.store(1, Ordering::Relaxed);
    let error = checked(
        &r,
        &font,
        "office ffi AV",
        "en",
        Direction::LeftToRight,
        &[],
    )
    .unwrap_err();
    assert_eq!(error, Error::native(-812));
    assert_eq!(r.failure(), Some(error));
    let before = state.reserves.load(Ordering::Relaxed);
    assert_eq!(Data::try_new(&font, &r).err(), Some(error));
    assert_eq!(state.reserves.load(Ordering::Relaxed), before);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
}

struct WidthProvider {
    calls: usize,
    fail: usize,
    observed_request: ResourcesRef,
}
impl FontFuncs for WidthProvider {
    fn advance_width(
        &mut self,
        b: &BuiltinFontFuncs<'_, '_>,
        g: crate::GlyphId,
    ) -> Result<i32, Error> {
        assert!(self.observed_request.is_same_request(b.resources()));
        self.calls += 1;
        if self.calls == self.fail {
            return Err(b.resources().record(Error::native(-830)));
        }
        b.advance_width(g)
    }
}

#[test]
fn real_shape_second_advance_window_stops_at_the_first_typed_callback_error() {
    let font = FontRef::from_index(TTC, 0).unwrap();
    let (r, state) = setup();
    let mut funcs = WidthProvider {
        calls: 0,
        fail: 257,
        observed_request: r.clone(),
    };
    {
        let data = Data::try_new(&font, &r).unwrap();
        let mut buffer = Buffer::try_new(&r).unwrap();
        buffer.try_push_str(&"A".repeat(600)).unwrap();
        buffer.try_guess_segment_properties().unwrap();
        let result = try_shape(
            &r,
            &data,
            None,
            buffer,
            ShapeOptions::new().request_font_funcs(Some(&mut funcs)),
            None,
        );
        assert_eq!(result.err(), Some(Error::native(-830)));
    }
    assert_eq!(funcs.calls, 257);
    drop(funcs);
    assert_eq!(r.failure(), Some(Error::native(-830)));
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

#[test]
fn auto_plan_last_natural_refund_is_checked_before_publishing_output() {
    let font = FontRef::from_index(TTC, 0).unwrap();
    let (r, state) = setup();
    let data = Data::try_new(&font, &r).unwrap();
    let mut buffer = Buffer::try_new(&r).unwrap();
    buffer.try_push_str("AV ffi office").unwrap();
    buffer.try_guess_segment_properties().unwrap();
    let before = state.releases.load(Ordering::Relaxed);
    let output = try_shape(&r, &data, None, buffer, ShapeOptions::new(), None).unwrap();
    let last = state.releases.load(Ordering::Relaxed);
    assert!(last > before);
    drop(output);
    drop(data);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();

    let (r, state) = setup();
    let data = Data::try_new(&font, &r).unwrap();
    let mut buffer = Buffer::try_new(&r).unwrap();
    buffer.try_push_str("AV ffi office").unwrap();
    buffer.try_guess_segment_properties().unwrap();
    state.fail_release.store(last, Ordering::Relaxed);
    let result = try_shape(&r, &data, None, buffer, ShapeOptions::new(), None);
    assert_eq!(result.err(), Some(Error::native(-812)));
    drop(data);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
    assert_eq!(r.failure(), Some(Error::native(-812)));
}

#[test]
fn original_inline_boundaries_spill_only_after_the_inline_extent() {
    for inline in [8usize, 11] {
        let (r, state) = setup();
        if inline == 8 {
            let mut values = Inline::<u8, 8>::new(&Control(Some(r.clone())));
            values.try_resize(8, 3).unwrap();
            assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
            values.try_push(4).unwrap();
            assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
            assert_eq!(&*values, &[3, 3, 3, 3, 3, 3, 3, 3, 4]);
            values.clear();
            assert!(state.live() > 0);
        } else {
            let mut values = Inline::<F2Dot14, 11>::new(&Control(Some(r.clone())));
            values.try_resize(11, F2Dot14::ZERO).unwrap();
            assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
            values.try_push(F2Dot14::ONE).unwrap();
            assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
            assert_eq!(values.len(), 12);
        }
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn paid_range_set_keeps_full_u32_endpoint_merge_and_intersection_semantics() {
    let (r, state) = setup();
    {
        let c = Control(Some(r.clone()));
        let mut a = crate::hb::storage::GlyphSet::new(&c);
        let mut b = crate::hb::storage::GlyphSet::new(&c);
        a.insert_range(u32::MAX - 2..=u32::MAX);
        a.insert_range(0..=4);
        a.insert_range(5..=9);
        a.insert_range(8..=17);
        a.insert_range(42..=41);
        assert!(a.contains(0) && a.contains(17) && a.contains(u32::MAX));
        assert!(!a.contains(18));
        b.insert(u32::MAX);
        assert!(a.intersects_set(&b));
        b.clear();
        b.insert(18);
        assert!(!a.intersects_set(&b));
        a.insert_range(18..=u32::MAX - 3);
        assert!(a.contains(1_000_000_000));
        assert!(a.intersects_set(&b));
    }
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

// These fonts stay in the Windows installation. The execution receipt records
// their exact bytes; the repository neither redistributes them nor assumes the
// same versions are installed on another machine.
#[cfg(windows)]
#[test]
fn installed_real_fonts_exercise_all_five_owned_script_data_and_match_every_word() {
    use crate::hb::ot_shape_plan::ScriptData;
    let windows = std::env::var_os("WINDIR").expect("Windows font test requires WINDIR");
    for (file, index, text, language, kind) in [
        ("Nirmala.ttc", 0, "कि क्त हिन्दी", "hi", "Indic"),
        ("LeelawUI.ttf", 0, "ខ្មែរ ក្ខ", "km", "Khmer"),
        ("Nirmala.ttc", 0, "සිංහල ක්ෂ", "si", "Universal"),
        ("malgun.ttf", 0, "각 한글 각", "ko", "Hangul"),
        ("segoeui.ttf", 0, "سلام العربية", "ar", "Arabic"),
    ] {
        let path = std::path::Path::new(&windows).join("Fonts").join(file);
        let bytes = std::fs::read(&path).unwrap();
        let font = FontRef::from_index(&bytes, index).unwrap();
        let direction = if kind == "Arabic" {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        let expected = ordinary(&font, text, language, direction, &[]);
        assert!(
            expected.0.iter().all(|g| g[0] != 0),
            "no mapped glyphs: {file}/{language}"
        );
        let (r, state) = setup();
        let data = Data::try_new(&font, &r).unwrap();
        let mut buffer = Buffer::try_new(&r).unwrap();
        buffer.try_push_str(text).unwrap();
        buffer.try_set_language(Some(language)).unwrap();
        buffer.try_set_direction(direction).unwrap();
        buffer.try_guess_segment_properties().unwrap();
        let plan = Plan::try_new(&data, None, &buffer, &[]).unwrap();
        let actual_kind = match plan.core.data.as_ref().unwrap() {
            ScriptData::Arabic(_) => "Arabic",
            ScriptData::Hangul(_) => "Hangul",
            ScriptData::Indic(_) => "Indic",
            ScriptData::Khmer(_) => "Khmer",
            ScriptData::Universal(_) => "Universal",
        };
        assert_eq!(actual_kind, kind);
        let out = try_shape(&r, &data, None, buffer, ShapeOptions::new(), Some(&plan)).unwrap();
        let actual: (std::vec::Vec<[u32; 5]>, std::vec::Vec<[u32; 5]>) = (
            bytemuck::cast_slice(out.glyph_infos()).to_vec(),
            bytemuck::cast_slice(out.glyph_positions()).to_vec(),
        );
        assert_eq!(actual, expected, "{file}/{language}/{kind}");
        drop(plan);
        drop(data);
        drop(out);
        r.checkpoint(0).unwrap();
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
        eprintln!(
            "actual script={kind}, file={}, bytes={}, glyphs={}",
            path.display(),
            bytes.len(),
            actual.0.len()
        );
    }
}

#[cfg(windows)]
#[test]
fn installed_complex_layout_every_actual_allocation_and_natural_refund_fault_stops() {
    let windows = std::env::var_os("WINDIR").unwrap();
    let bytes = std::fs::read(
        std::path::Path::new(&windows)
            .join("Fonts")
            .join("Nirmala.ttc"),
    )
    .unwrap();
    let font = FontRef::from_index(&bytes, 0).unwrap();
    let text = "कि क्त हिन्दी";
    let (r, state) = setup();
    checked(&r, &font, text, "hi", Direction::LeftToRight, &[]).unwrap();
    let counts = [
        state.reserves.load(Ordering::Relaxed),
        state.commits.load(Ordering::Relaxed),
        state.releases.load(Ordering::Relaxed),
    ];
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    assert!(counts[0] > 12);
    for (phase, &count) in counts.iter().enumerate() {
        for ordinal in 1..=count {
            let (r, state) = setup();
            match phase {
                0 => state.fail_reserve.store(ordinal, Ordering::Relaxed),
                1 => state.fail_commit.store(ordinal, Ordering::Relaxed),
                _ => state.fail_release.store(ordinal, Ordering::Relaxed),
            }
            let expected = Error::native([-810, -811, -812][phase]);
            assert_eq!(
                checked(&r, &font, text, "hi", Direction::LeftToRight, &[]).unwrap_err(),
                expected,
                "phase={phase},ordinal={ordinal}"
            );
            assert_eq!(r.failure(), Some(expected));
            if phase == 2 {
                assert_eq!(state.entries.lock().unwrap().len(), 1);
                assert_eq!(r.statistics().release_failures, 1);
            } else {
                assert_eq!(state.live(), 0);
                r.validate_retired().unwrap();
            }
        }
    }
    eprintln!("Nirmala actual layout fault matrix: reserve={},commit={},naturalRefund={}; every ordinal hit",counts[0],counts[1],counts[2]);
}

fn repair_minimal_font(chains: Option<u32>) -> std::vec::Vec<u8> {
    let mut bytes = vec![0, 1, 0, 0, 0, u8::from(chains.is_some()), 0, 0, 0, 0, 0, 0];
    if let Some(n) = chains {
        bytes.extend_from_slice(b"morx");
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&28u32.to_be_bytes());
        bytes.extend_from_slice(&(8 + n * 16).to_be_bytes());
        bytes.extend_from_slice(&[0, 2, 0, 0]);
        bytes.extend_from_slice(&n.to_be_bytes());
        for _ in 0..n {
            bytes.extend_from_slice(&0u32.to_be_bytes());
            bytes.extend_from_slice(&16u32.to_be_bytes());
            bytes.extend_from_slice(&0u32.to_be_bytes());
            bytes.extend_from_slice(&0u32.to_be_bytes());
        }
    }
    bytes
}

fn repair_shape_features(
    r: &ResourcesRef,
    font: &FontRef<'_>,
    features: &[Feature],
) -> Result<(std::vec::Vec<[u32; 5]>, std::vec::Vec<[u32; 5]>), Error> {
    let result = {
        let data = Data::try_new(font, r)?;
        let mut buffer = Buffer::try_new(r)?;
        buffer.try_push_str("abcde abcde")?;
        buffer.try_set_direction(Direction::LeftToRight)?;
        buffer.try_guess_segment_properties()?;
        let output = try_shape(
            r,
            &data,
            None,
            buffer,
            ShapeOptions::new().features(features),
            None,
        )?;
        let value = (
            bytemuck::cast_slice(output.glyph_infos()).to_vec(),
            bytemuck::cast_slice(output.glyph_positions()).to_vec(),
        );
        drop(output);
        drop(data);
        value
    };
    r.checkpoint(0)?;
    Ok(result)
}

#[test]
fn repair_nonempty_aat_features_real_outputs_and_each_funded_owner_fault() {
    let features = [
        Feature {
            tag: crate::Tag::new(b"kern"),
            value: 1,
            start: 0,
            end: u32::MAX,
        },
        Feature {
            tag: crate::Tag::new(b"liga"),
            value: 0,
            start: 0,
            end: 3,
        },
        Feature {
            tag: crate::Tag::new(b"smcp"),
            value: 1,
            start: 3,
            end: 9,
        },
    ];
    let minimal = repair_minimal_font(Some(1));
    for bytes in [
        &*minimal,
        AAT_NONCONTEXT,
        AAT_REARRANGE,
        AAT_CONTEXT,
        AAT_LIGATURE,
    ] {
        let font = FontRef::from_index(bytes, 0).unwrap();
        let ordinary_data = ShaperData::new(&font);
        let face = ordinary_data.shaper(&font).build();
        let mut input = UnicodeBuffer::new();
        input.push_str("abcde abcde");
        input.set_direction(Direction::LeftToRight);
        input.guess_segment_properties();
        let ordinary = face.shape(input, ShapeOptions::new().features(&features));
        let expected: (std::vec::Vec<[u32; 5]>, std::vec::Vec<[u32; 5]>) = (
            bytemuck::cast_slice(ordinary.glyph_infos()).to_vec(),
            bytemuck::cast_slice(ordinary.glyph_positions()).to_vec(),
        );
        let (r, state) = setup();
        assert_eq!(
            repair_shape_features(&r, &font, &features).unwrap(),
            expected
        );
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
    let font = FontRef::from_index(&minimal, 0).unwrap();
    let (r, state) = setup();
    repair_shape_features(&r, &font, &features).unwrap();
    let counts = [
        state.reserves.load(Ordering::Relaxed),
        state.commits.load(Ordering::Relaxed),
        state.releases.load(Ordering::Relaxed),
    ];
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
    for (phase, &count) in counts.iter().enumerate() {
        for ordinal in 1..=count {
            let (r, state) = setup();
            match phase {
                0 => state.fail_reserve.store(ordinal, Ordering::Relaxed),
                1 => state.fail_commit.store(ordinal, Ordering::Relaxed),
                _ => state.fail_release.store(ordinal, Ordering::Relaxed),
            };
            let expected = Error::native([-810, -811, -812][phase]);
            assert_eq!(
                repair_shape_features(&r, &font, &features).unwrap_err(),
                expected,
                "phase={phase},ordinal={ordinal}"
            );
            assert_eq!(r.failure(), Some(expected));
            if phase == 2 {
                assert_eq!(state.entries.lock().unwrap().len(), 1);
                assert_eq!(r.statistics().release_failures, 1);
            } else {
                assert_eq!(state.live(), 0);
                r.validate_retired().unwrap();
            }
        }
    }
    eprintln!("nonempty AAT feature actual owner faults: reserve={},commit={},naturalRefund={}; every ordinal hit",counts[0],counts[1],counts[2]);
}

#[test]
fn repair_nonvariable_owned_iterator_retires_before_instance_success() {
    struct OwnedVars {
        _bytes: markdown_svg_budget::BudgetVec<u8>,
    }
    impl Iterator for OwnedVars {
        type Item = Variation;
        fn next(&mut self) -> Option<Variation> {
            None
        }
    }
    let font = FontRef::from_index(TTC, 0).unwrap();
    let (r, state) = setup();
    let data = Data::try_new(&font, &r).unwrap();
    let mut bytes = markdown_svg_budget::BudgetVec::new(
        r.clone(),
        markdown_svg_budget::ResourceKind::Scratch,
    );
    bytes.try_reserve_exact(1).unwrap();
    bytes.push_reserved(7);
    state.fail_release.store(
        state.releases.load(Ordering::Relaxed) + 1,
        Ordering::Relaxed,
    );
    assert_eq!(
        Instance::try_from_variations(&data, OwnedVars { _bytes: bytes }).err(),
        Some(Error::native(-812))
    );
    drop(data);
    assert_eq!(r.failure(), Some(Error::native(-812)));
    assert_eq!(state.entries.lock().unwrap().len(), 1);
    assert_eq!(r.statistics().release_failures, 1);
}

#[test]
fn repair_long_canonical_language_and_1024_real_morx_chains_retire() {
    let bytes = repair_minimal_font(Some(1024));
    let font = FontRef::from_index(&bytes, 0).unwrap();
    let (r, state) = setup();
    {
        let data = Data::try_new(&font, &r).unwrap();
        let mut buffer = Buffer::try_new(&r).unwrap();
        buffer.try_set_direction(Direction::LeftToRight).unwrap();
        buffer
            .try_set_language(Some(&format!("en-{}", "a".repeat(65536))))
            .unwrap();
        let plan = Plan::try_new(&data, None, &buffer, &[]).unwrap();
        assert_eq!(plan.core.aat_map.chain_flags.len(), 1024);
        assert!(plan
            .core
            .aat_map
            .chain_flags
            .iter()
            .all(|f| f.last().unwrap().cluster_last == u32::MAX));
    }
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

fn repair_aalt_font() -> std::vec::Vec<u8> {
    let mut bytes = vec![0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
    for (tag, offset, len) in [(b"feat", 44u32, 28u32), (b"morx", 72u32, 24u32)] {
        bytes.extend_from_slice(tag);
        bytes.extend_from_slice(&0u32.to_be_bytes());
        bytes.extend_from_slice(&offset.to_be_bytes());
        bytes.extend_from_slice(&len.to_be_bytes());
    }
    bytes.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
    bytes.extend_from_slice(&17u16.to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&24u32.to_be_bytes());
    bytes.extend_from_slice(&0x8000u16.to_be_bytes());
    bytes.extend_from_slice(&0u16.to_be_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&[0, 2, 0, 0]);
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&16u32.to_be_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes
}

#[test]
fn repair_aalt_u32_boundary_records_capacity_before_callbacks_and_next_run() {
    #[derive(Default)]
    struct Calls(usize);
    impl FontFuncs for Calls {
        fn advance_width(
            &mut self,
            b: &BuiltinFontFuncs<'_, '_>,
            g: crate::GlyphId,
        ) -> Result<i32, Error> {
            self.0 += 1;
            b.advance_width(g)
        }
    }
    let bytes = repair_aalt_font();
    let font = FontRef::from_index(&bytes, 0).unwrap();
    for value in [65535, 65536, u32::MAX] {
        let features = [Feature {
            tag: crate::Tag::new(b"aalt"),
            value,
            start: 0,
            end: u32::MAX,
        }];
        let (r, state) = setup();
        let data = Data::try_new(&font, &r).unwrap();
        let mut buffer = Buffer::try_new(&r).unwrap();
        buffer.try_push_str("a").unwrap();
        buffer.try_set_direction(Direction::LeftToRight).unwrap();
        let mut calls = Calls::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            try_shape(
                &r,
                &data,
                None,
                buffer,
                ShapeOptions::new()
                    .features(&features)
                    .request_font_funcs(Some(&mut calls)),
                None,
            )
        }));
        assert!(
            result.is_ok(),
            "checked aalt={value} must return a typed result"
        );
        let result = result.unwrap();
        if value == 65535 {
            let output = result.unwrap();
            let ordinary_data = ShaperData::new(&font);
            let face = ordinary_data.shaper(&font).build();
            let mut input = UnicodeBuffer::new();
            input.push_str("a");
            input.set_direction(Direction::LeftToRight);
            let ordinary = face.shape(input, ShapeOptions::new().features(&features));
            assert_eq!(
                bytemuck::cast_slice::<_, u32>(output.glyph_infos()),
                bytemuck::cast_slice::<_, u32>(ordinary.glyph_infos())
            );
            assert_eq!(
                bytemuck::cast_slice::<_, u32>(output.glyph_positions()),
                bytemuck::cast_slice::<_, u32>(ordinary.glyph_positions())
            );
            assert!(calls.0 > 0);
            drop(output);
        } else {
            let expected = Error::new(FailureKind::Quota, LimitKind::Capacity);
            assert_eq!(result.err(), Some(expected));
            assert_eq!(r.failure(), Some(expected));
            assert_eq!(calls.0, 0);
            let before = state.reserves.load(Ordering::Relaxed);
            assert_eq!(Data::try_new(&font, &r).err(), Some(expected));
            assert_eq!(state.reserves.load(Ordering::Relaxed), before);
        }
        drop(data);
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn repair_plain_aalt_keeps_the_original_over_u16_panic() {
    let bytes = repair_aalt_font();
    let font = FontRef::from_index(&bytes, 0).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let data = ShaperData::new(&font);
        let face = data.shaper(&font).build();
        let mut input = UnicodeBuffer::new();
        input.push_str("a");
        input.set_direction(Direction::LeftToRight);
        face.shape(
            input,
            ShapeOptions::new().features(&[Feature {
                tag: crate::Tag::new(b"aalt"),
                value: 65536,
                start: 0,
                end: u32::MAX,
            }]),
        );
    }));
    assert!(
        result.is_err(),
        "ordinary AAT narrowing keeps its historical behavior"
    );
}
