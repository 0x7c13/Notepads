// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Checked reader transport and natural scratch retirement regressions.
use crate::{
    control::*,
    ps::{
        cff::{dict, index::Index},
        cs::{self, CommandSink, RequestCffContext},
    },
    tables::{avar::Avar, fvar::Fvar, variations::VariationRegion},
    *,
};
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket, WorkControl,
};
use std::sync::{Arc, Mutex};
use types::Scalar;

#[derive(Default)]
struct Steps {
    work: Vec<u64>,
    stop: Option<usize>,
}
impl WorkControl for Steps {
    type Error = usize;
    fn checkpoint(&mut self, work: u64) -> Result<(), usize> {
        self.work.push(work);
        let ordinal = self.work.len();
        if self.stop == Some(ordinal) {
            Err(ordinal)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Reserve,
    Commit,
    Release,
    Work,
}
#[derive(Default)]
struct Ledger {
    events: Vec<(Call, u64)>,
    next: u32,
    live: Vec<(u32, usize)>,
    fault: Option<(Call, usize, Error)>,
}
impl Ledger {
    fn event(&mut self, call: Call, value: u64) -> Result<(), Error> {
        self.events.push((call, value));
        let count = self.events.iter().filter(|e| e.0 == call).count();
        if let Some((at, ordinal, error)) = self.fault {
            if call == at && count == ordinal {
                return Err(error);
            }
        }
        Ok(())
    }
    fn count(&self, call: Call) -> usize {
        self.events.iter().filter(|e| e.0 == call).count()
    }
}
#[derive(Clone)]
struct Host(Arc<Mutex<Ledger>>);
impl Backend for Host {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        assert_eq!(kind, ResourceKind::Scratch);
        let mut s = self.0.lock().unwrap();
        s.event(Call::Reserve, bytes as u64)?;
        s.next += 1;
        let id = s.next;
        s.live.push((id, bytes));
        Ticket::new(0x5246, id, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut s = self.0.lock().unwrap();
        assert!(s.live.contains(&(ticket.slot(), bytes)));
        s.event(Call::Commit, bytes as u64)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut s = self.0.lock().unwrap();
        let i = s.live.iter().position(|e| e.0 == ticket.slot()).unwrap();
        let (_, bytes) = s.live.remove(i);
        s.event(Call::Release, bytes as u64)
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.lock().unwrap().event(Call::Work, work)
    }
}
fn resources() -> (ResourcesRef, Arc<Mutex<Ledger>>) {
    let log = Arc::new(Mutex::new(Ledger::default()));
    (
        ResourcesRef::new_prepaid(
            Host(log.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        log,
    )
}
fn failure() -> Error {
    Error::new(FailureKind::Cancelled, LimitKind::None)
}
fn assert_retired(r: &ResourcesRef, log: &Arc<Mutex<Ledger>>) {
    assert!(log.lock().unwrap().live.is_empty());
    assert_eq!(r.statistics().live_leases, 0);
    assert_eq!(r.statistics().scratch_bytes, 0);
}
fn fvar_bytes(count: u16) -> Vec<u8> {
    let mut bytes = Vec::from([0, 1, 0, 0, 0, 16, 0, 2]);
    bytes.extend(count.to_be_bytes());
    bytes.extend(20u16.to_be_bytes());
    bytes.extend(0u16.to_be_bytes());
    bytes.extend((4 + count * 4).to_be_bytes());
    for _ in 0..count {
        bytes.extend(b"wght");
        bytes.extend(0i32.to_be_bytes());
        bytes.extend(0i32.to_be_bytes());
        bytes.extend(65536i32.to_be_bytes());
        bytes.extend([0, 0, 1, 0]);
    }
    bytes
}
fn avar_bytes(count: u16, version: u8, mapping_count: u16) -> Vec<u8> {
    let mut bytes = Vec::from([0, version, 0, 0, 0, 0]);
    bytes.extend(count.to_be_bytes());
    for _ in 0..count {
        bytes.extend(mapping_count.to_be_bytes());
        for _ in 0..mapping_count {
            bytes.extend([0, 0, 0, 0]);
        }
    }
    if version == 2 {
        bytes.extend([0; 8]);
    }
    bytes
}

#[test]
fn original_font_directories_and_all_table_bytes_match_checked() {
    for bytes in [
        font_test_data::AHEM,
        font_test_data::VAZIRMATN_VAR,
        font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
        font_test_data::AVAR2_CHECKER,
    ] {
        let plain = FontRef::new(bytes).unwrap();
        let mut cx = Steps::default();
        let checked = FontRef::try_new(bytes, &mut cx).unwrap().unwrap();
        assert_eq!(plain.data().as_bytes(), checked.data().as_bytes());
        let new_steps = cx.work.len();
        for record in plain.table_directory().table_records() {
            let tag = record.tag();
            assert_eq!(
                plain.table_data(tag).map(|d| d.as_bytes()),
                checked
                    .try_table_data(tag, &mut cx)
                    .unwrap()
                    .map(|d| d.as_bytes())
            );
        }
        for at in 1..=new_steps {
            let mut fail = Steps {
                stop: Some(at),
                ..Default::default()
            };
            assert_eq!(FontRef::try_new(bytes, &mut fail).err(), Some(at));
            assert_eq!(fail.work, &cx.work[..at]);
        }
    }
}
#[test]
fn checked_bad_collection_count_is_inner_font_error() {
    let bytes = [b't', b't', b'c', b'f', 0, 1, 0, 0, 255, 255, 255, 255];
    let mut cx = Steps::default();
    let result = FontRef::try_from_index(&bytes, 0, &mut cx).unwrap();
    assert!(result.is_err());
}
#[test]
fn nullable_offset_and_cursor_preserve_null_font_eof_and_control() {
    use crate::tables::head::Head;
    let data = FontData::new(&[0; 64]);
    let mut cx = Steps::default();
    let null = Nullable::<Offset16>::from_raw(0u16.to_be_bytes());
    let bad = Nullable::<Offset16>::from_raw(100u16.to_be_bytes());
    assert!(
        TryResolveNullableOffset::try_resolve::<Head, _>(&null, data, &mut cx)
            .unwrap()
            .is_none()
    );
    assert!(
        TryResolveNullableOffset::try_resolve::<Head, _>(&bad, data, &mut cx)
            .unwrap()
            .unwrap()
            .is_err()
    );
    let mut stop = Steps {
        stop: Some(1),
        ..Default::default()
    };
    assert_eq!(
        TryResolveNullableOffset::try_resolve::<Head, _>(&null, data, &mut stop).err(),
        Some(1)
    );
    let offsets = [
        types::BigEndian::new(0u16.to_be_bytes()),
        types::BigEndian::new(100u16.to_be_bytes()),
    ];
    let array = ArrayOfNullableOffsets::<Head, Offset16>::new(&offsets, data, ());
    let mut cursor = array.try_iter();
    assert!(cursor.try_next(&mut cx).unwrap().unwrap().is_none());
    assert!(cursor.try_next(&mut cx).unwrap().unwrap().unwrap().is_err());
    assert!(cursor.try_next(&mut cx).unwrap().is_none());
}
#[test]
fn generated_avar_prescan_and_second_256_window_stop_with_exact_error() {
    let bytes = avar_bytes(300, 2, 1);
    let avar = Avar::read(bytes.as_slice().into()).unwrap();
    let mut baseline = Steps::default();
    let range = avar
        .try_axis_segment_maps_byte_range(&mut baseline)
        .unwrap();
    assert_eq!(range, 8..1808);
    assert_eq!(avar.axis_segment_maps_byte_range(), range);
    let index = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(index),
        ..Default::default()
    };
    assert_eq!(avar.try_axis_segment_maps_byte_range(&mut fail), Err(index));
    assert_eq!(fail.work, &baseline.work[..index]);
    let mut one = Steps::default();
    Avar::try_read(bytes.as_slice().into(), &mut one)
        .unwrap()
        .unwrap();
    assert!(one.work.len() < 10);
}
#[test]
fn zero_peak_axes_are_still_charged_and_cancelled_before_next_axis() {
    let bytes = vec![0u8; 300 * 6];
    let region = VariationRegion::read(bytes.as_slice().into(), 300).unwrap();
    let mut baseline = Steps::default();
    assert_eq!(
        region.try_compute_scalar(&[], &mut baseline).unwrap(),
        types::Fixed::ONE
    );
    assert_eq!(
        region
            .try_compute_scalar_f32(&[], &mut Steps::default())
            .unwrap()
            .to_bits(),
        region.compute_scalar_f32(&[]).to_bits()
    );
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(region.try_compute_scalar(&[], &mut fail), Err(at));
    assert_eq!(fail.work, &baseline.work[..at]);
}
#[test]
fn real_variable_fonts_normalize_bit_identically() {
    for bytes in [
        font_test_data::VAZIRMATN_VAR,
        font_test_data::AVAR2_CHECKER,
        font_test_data::AMSTELVAR_AVAR2_A,
        font_test_data::CANTARELL_VF_TRIMMED,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let fvar = font.fvar().unwrap();
        let avar = font.avar().ok();
        let axes = fvar.axes().unwrap();
        for value in [
            types::Fixed::NEG_ONE,
            types::Fixed::ZERO,
            types::Fixed::ONE,
            types::Fixed::from_i32(500),
        ] {
            let user: Vec<_> = axes.iter().map(|axis| (axis.axis_tag(), value)).collect();
            let mut ordinary = vec![types::F2Dot14::ZERO; axes.len() + 2];
            let mut checked = ordinary.clone();
            fvar.user_to_normalized(avar.as_ref(), user.iter().copied(), &mut ordinary);
            let (r, log) = resources();
            fvar.try_user_to_normalized(avar.as_ref(), user.iter().copied(), &mut checked, &r)
                .unwrap();
            assert_eq!(ordinary, checked);
            assert_retired(&r, &log);
        }
    }
}
#[test]
fn normalization_spills_are_paid_and_every_reserve_commit_failure_retires() {
    let bytes = fvar_bytes(65);
    let fvar = Fvar::read(bytes.as_slice().into()).unwrap();
    let avar_data = avar_bytes(65, 2, 0);
    let avar = Avar::read(avar_data.as_slice().into()).unwrap();
    for fault in [
        None,
        Some((Call::Reserve, 1)),
        Some((Call::Reserve, 2)),
        Some((Call::Commit, 1)),
        Some((Call::Commit, 2)),
        Some((Call::Release, 1)),
        Some((Call::Release, 2)),
    ] {
        let (r, log) = resources();
        if let Some((call, n)) = fault {
            log.lock().unwrap().fault = Some((call, n, failure()));
        }
        let mut output = [types::F2Dot14::ZERO; 65];
        let result = fvar.try_user_to_normalized(
            Some(&avar),
            [(types::Tag::new(b"wght"), types::Fixed::ONE)],
            &mut output,
            &r,
        );
        if fault.is_some() {
            assert_eq!(result, Err(failure()));
            assert_eq!(r.failure(), Some(failure()));
        } else {
            result.unwrap();
            assert_eq!(output, [types::F2Dot14::ONE; 65]);
            let s = log.lock().unwrap();
            assert_eq!(s.count(Call::Reserve), 2);
            assert_eq!(s.count(Call::Commit), 2);
            assert_eq!(s.count(Call::Release), 2);
        }
        if fault == Some((Call::Release, 1)) {
            assert_eq!(output, [types::F2Dot14::ZERO; 65]);
        }
        if matches!(fault, Some((Call::Release, _))) {
            assert!(log.lock().unwrap().live.is_empty());
            assert_eq!(r.statistics().release_failures, 1);
            assert_eq!(r.statistics().live_leases, 1);
            assert!(r.statistics().scratch_bytes > 0);
            assert_eq!(r.validate_retired(), Err(failure()));
        } else {
            assert_retired(&r, &log);
        }
    }
}
#[test]
fn normalization_second_axis_work_cancellation_has_no_later_work_or_alloc() {
    let bytes = fvar_bytes(300);
    let fvar = Fvar::read(bytes.as_slice().into()).unwrap();
    let (r, log) = resources();
    let mut output = vec![types::F2Dot14::ZERO; 300];
    fvar.try_user_to_normalized(
        None,
        [(types::Tag::new(b"wght"), types::Fixed::ONE)],
        &mut output,
        &r,
    )
    .unwrap();
    let events = log.lock().unwrap().events.clone();
    let target = events
        .iter()
        .filter(|e| e.0 == Call::Work && e.1 == 1)
        .nth(256)
        .copied()
        .unwrap();
    assert_eq!(target, (Call::Work, 1));
    let at = events
        .iter()
        .enumerate()
        .filter(|e| e.1 .0 == Call::Work && e.1 .1 == 1)
        .nth(256)
        .unwrap()
        .0;
    let ordinal = events[..=at].iter().filter(|e| e.0 == Call::Work).count();
    let (r, log) = resources();
    log.lock().unwrap().fault = Some((Call::Work, ordinal, failure()));
    assert_eq!(
        fvar.try_user_to_normalized(
            None,
            [(types::Tag::new(b"wght"), types::Fixed::ONE)],
            &mut output,
            &r
        ),
        Err(failure())
    );
    let observed = log.lock().unwrap().events.clone();
    assert_eq!(&observed[..=at], &events[..=at]);
    assert!(observed[at + 1..].iter().all(|e| e.0 == Call::Release));
    assert_retired(&r, &log);
}
fn matrix_bytes() -> Vec<u8> {
    let mut bytes = vec![30];
    bytes.extend([0u8; 300]);
    bytes.extend([0xA0, 0x01, 0xF0]);
    bytes.extend([139, 139, 30, 0x0A, 0x00, 0x1F, 139, 139, 12, 7]);
    bytes
}
#[test]
fn bcd_long_single_token_and_fontmatrix_reparse_stop_inside_each_pass() {
    let bytes = matrix_bytes();
    let mut baseline = Steps::default();
    let mut entries = dict::try_entries(&bytes, None);
    let value = entries.try_next(&mut baseline).unwrap().unwrap().unwrap();
    assert!(matches!(value, dict::Entry::FontMatrix(_)));
    assert!(baseline.work.iter().filter(|v| **v == 2).count() > 600);
    let mut ordinary = dict::entries(&bytes, None);
    assert_eq!(
        format!("{value:?}"),
        format!("{:?}", ordinary.next().unwrap().unwrap())
    );
    let indices: Vec<_> = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 2)
        .map(|e| e.0 + 1)
        .collect();
    for at in [indices[256], indices[indices.len() - 260]] {
        let mut fail = Steps {
            stop: Some(at),
            ..Default::default()
        };
        let mut entries = dict::try_entries(&bytes, None);
        assert_eq!(entries.try_next(&mut fail).err(), Some(at));
        assert_eq!(fail.work, &baseline.work[..at]);
    }
    let mut token = vec![30];
    token.extend([0u8; 4096]);
    token.push(0x1F);
    let mut tokens = dict::try_tokens(&token);
    let mut fail = Steps {
        stop: Some(260),
        ..Default::default()
    };
    assert_eq!(tokens.try_next(&mut fail).err(), Some(260));
    assert_eq!(fail.work.len(), 260);
}
#[derive(Default, Debug, PartialEq)]
struct Commands {
    points: Vec<(u8, Vec<i32>)>,
    finishes: usize,
}
impl CommandSink for Commands {
    fn move_to(&mut self, x: types::Fixed, y: types::Fixed) {
        self.points.push((0, vec![x.to_bits(), y.to_bits()]));
    }
    fn line_to(&mut self, x: types::Fixed, y: types::Fixed) {
        self.points.push((1, vec![x.to_bits(), y.to_bits()]));
    }
    fn curve_to(
        &mut self,
        a: types::Fixed,
        b: types::Fixed,
        c: types::Fixed,
        d: types::Fixed,
        e: types::Fixed,
        f: types::Fixed,
    ) {
        self.points
            .push((2, [a, b, c, d, e, f].map(|v| v.to_bits()).to_vec()));
    }
    fn close(&mut self) {
        self.points.push((3, vec![]));
    }
    fn finish(&mut self) {
        self.finishes += 1;
    }
}
#[test]
fn charstring_no_pen_reserved_loop_and_callback_fault_stop_before_next_action() {
    let empty = Index::Empty;
    let (r, log) = resources();
    let ctx = RequestCffContext::new(&[], &empty, &empty, &empty, &r);
    let mut sink = Commands::default();
    let bytes = vec![0u8; 4096];
    log.lock().unwrap().fault = Some((Call::Work, 258, failure()));
    assert_eq!(
        cs::try_evaluate(&ctx, None, &bytes, &mut sink).err(),
        Some(failure())
    );
    assert_eq!(log.lock().unwrap().count(Call::Work), 258);
    assert!(sink.points.is_empty());
    assert_eq!(sink.finishes, 0);
    struct FaultSink<'a> {
        r: &'a ResourcesRef,
        calls: usize,
    }
    impl CommandSink for FaultSink<'_> {
        fn move_to(&mut self, _: types::Fixed, _: types::Fixed) {
            self.calls += 1;
            self.r.record(failure());
        }
        fn line_to(&mut self, _: types::Fixed, _: types::Fixed) {
            self.calls += 1;
        }
        fn curve_to(
            &mut self,
            _: types::Fixed,
            _: types::Fixed,
            _: types::Fixed,
            _: types::Fixed,
            _: types::Fixed,
            _: types::Fixed,
        ) {
            self.calls += 1;
        }
        fn close(&mut self) {
            self.calls += 1;
        }
        fn finish(&mut self) {
            self.calls += 1;
        }
    }
    let (r, _) = resources();
    let ctx = RequestCffContext::new(&[], &empty, &empty, &empty, &r);
    let mut sink = FaultSink { r: &r, calls: 0 };
    assert_eq!(
        cs::try_evaluate(&ctx, None, &[139, 139, 21, 140, 140, 5, 14], &mut sink).err(),
        Some(failure())
    );
    assert_eq!(sink.calls, 1);
}
#[test]
fn real_charstring_geometry_and_width_match_original_context() {
    let font = FontRef::new(font_test_data::CHARSTRING_PATH_OPS).unwrap();
    let cff = font.cff().unwrap();
    let data = cff.offset_data().as_bytes();
    let cff_font = crate::ps::cff::CffFontRef::new(data, 0, None).unwrap();
    let chars = cff_font.charstrings();
    let global = cff_font.global_subrs();
    let local = Index::Empty;
    let mut successful = 0;
    for gid in 0..chars.count() {
        let bytes = chars.get(gid as usize).unwrap();
        let mut ordinary = Commands::default();
        let result = cs::evaluate(&(data, chars, global, &local), None, bytes, &mut ordinary);
        let (r, log) = resources();
        let ctx = RequestCffContext::new(data, chars, global, &local, &r);
        let mut checked = Commands::default();
        let controlled = cs::try_evaluate(&ctx, None, bytes, &mut checked).unwrap();
        assert_eq!(format!("{result:?}"), format!("{controlled:?}"));
        assert_eq!(ordinary, checked);
        if result.is_ok() {
            successful += 1;
        }
        assert_retired(&r, &log);
    }
    assert!(successful > 0);
}

#[test]
fn real_cmap_mapping_variant_and_metric_values_match_every_checked_sibling() {
    for bytes in [
        font_test_data::AHEM,
        font_test_data::CMAP6,
        font_test_data::CMAP10,
        font_test_data::CMAP12_FONT1,
        font_test_data::CMAP14_FONT1,
        font_test_data::VAZIRMATN_VAR,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let cmap = font.cmap().unwrap();
        let mut cx = Steps::default();
        assert_eq!(
            cmap.best_subtable().map(|v| v.0),
            cmap.try_best_subtable(&mut cx).unwrap().map(|v| v.0)
        );
        let checked = cmap.try_uvs_subtable(&mut cx).unwrap();
        assert_eq!(
            cmap.uvs_subtable().map(|v| v.0),
            checked.as_ref().map(|v| v.0)
        );
        for codepoint in (0..1024u32).chain([0xFFFF, 0x10000, 0x10FFFF]) {
            assert_eq!(
                cmap.map_codepoint(codepoint),
                cmap.try_map_codepoint(codepoint, &mut cx).unwrap()
            );
            if let Some((_, cmap14)) = checked.as_ref() {
                for selector in [0xFE0Fu32, 0xE0100] {
                    assert_eq!(
                        cmap14.map_variant(codepoint, selector),
                        cmap14
                            .try_map_variant(codepoint, selector, &mut cx)
                            .unwrap()
                    );
                }
            }
        }
        if let Ok(hmtx) = font.hmtx() {
            for gid in 0..8 {
                let gid = types::GlyphId::new(gid);
                assert_eq!(hmtx.advance(gid), hmtx.try_advance(gid, &mut cx).unwrap());
                assert_eq!(
                    hmtx.side_bearing(gid),
                    hmtx.try_side_bearing(gid, &mut cx).unwrap()
                );
            }
        }
        if let Ok(hvar) = font.hvar() {
            for value in [
                types::F2Dot14::NEG_ONE,
                types::F2Dot14::ZERO,
                types::F2Dot14::ONE,
            ] {
                for gid in 0..8 {
                    let gid = types::GlyphId::new(gid);
                    assert_eq!(
                        hvar.advance_width_delta(gid, &[value]),
                        hvar.try_advance_width_delta(gid, &[value], &mut cx)
                            .unwrap()
                    );
                }
            }
        }
    }
}
#[test]
fn cmap_long_priority_scan_stops_in_second_256_window() {
    let mut bytes = Vec::from([0, 0, 1, 44]);
    let offset = 4 + 300 * 8u32;
    for _ in 0..300 {
        bytes.extend([0, 0, 0, 0]);
        bytes.extend(offset.to_be_bytes());
    }
    bytes.extend([0, 0, 1, 6, 0, 0]);
    bytes.extend([0; 256]);
    let cmap = crate::tables::cmap::Cmap::read(bytes.as_slice().into()).unwrap();
    let mut baseline = Steps::default();
    assert!(cmap.try_best_subtable(&mut baseline).unwrap().is_some());
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(cmap.try_best_subtable(&mut fail).err(), Some(at));
    assert_eq!(fail.work, &baseline.work[..at]);
}

#[test]
fn valid_repeat_255_yields_exactly_256_points_in_both_readers() {
    use crate::tables::glyf::{CurvePoint, PointFlags, SimpleGlyph};
    // One contour ends at point255. A repeat count255 means256 identical
    // on-curve flags, with unchanged x/y and no coordinate bytes.
    let mut bytes = Vec::from([0u8, 1]);
    bytes.extend([0; 8]);
    bytes.extend(255u16.to_be_bytes());
    bytes.extend(0u16.to_be_bytes());
    bytes.extend([0x39, 255]);
    let glyph = SimpleGlyph::read(bytes.as_slice().into()).unwrap();
    assert_eq!(glyph.num_points(), 256);
    let expected = vec![CurvePoint::on_curve(0, 0); 256];
    assert_eq!(glyph.points().collect::<Vec<_>>(), expected);
    let mut cursor = glyph.try_points(&mut Steps::default()).unwrap();
    let mut values = Vec::new();
    while let Some(point) = cursor.try_next(&mut Steps::default()).unwrap() {
        values.push(point);
    }
    assert_eq!(values, expected);
    let mut points = vec![types::Point::<i32>::default(); 256];
    let mut flags = vec![PointFlags::default(); 256];
    glyph
        .try_read_points_fast(&mut points, &mut flags, &mut Steps::default())
        .unwrap()
        .unwrap();
    assert!(points.iter().all(|p| p.x == 0 && p.y == 0));
    assert!(flags.iter().all(|f| f.is_on_curve()));
}

#[test]
fn real_glyf_components_points_and_gvar_tuple_streams_match_checked() {
    use crate::tables::glyf::{Glyph, PointFlags};
    let mut varied = 0;
    for bytes in [
        font_test_data::SIMPLE_GLYF,
        font_test_data::CUBIC_GLYF,
        font_test_data::GLYF_COMPONENTS,
        font_test_data::VAZIRMATN_VAR,
        font_test_data::INTERPOLATE_THIS,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let glyf = font.glyf().unwrap();
        let loca = font.loca(None).unwrap();
        let gvar = font.gvar().ok();
        let num = font.maxp().unwrap().num_glyphs();
        for id in 0..num {
            let gid = types::GlyphId::new(id as u32);
            let mut cx = Steps::default();
            let ordinary = loca.get_glyf(gid, &glyf).unwrap();
            let checked = loca.try_get_glyf(gid, &glyf, &mut cx).unwrap().unwrap();
            assert_eq!(ordinary.is_some(), checked.is_some());
            let point_count = match ordinary {
                Some(Glyph::Simple(simple)) => {
                    let count = simple.num_points();
                    let mut points = vec![types::Point::<i32>::default(); count];
                    let mut flags = vec![PointFlags::default(); count];
                    let mut out = points.clone();
                    let mut out_flags = flags.clone();
                    simple.read_points_fast(&mut points, &mut flags).unwrap();
                    simple
                        .try_read_points_fast(&mut out, &mut out_flags, &mut cx)
                        .unwrap()
                        .unwrap();
                    assert_eq!(points, out);
                    assert_eq!(flags, out_flags);
                    let ordinary: Vec<_> = simple.points().collect();
                    let mut cursor = simple.try_points(&mut cx).unwrap();
                    let mut controlled = Vec::new();
                    while let Some(p) = cursor.try_next(&mut cx).unwrap() {
                        controlled.push(p);
                    }
                    assert_eq!(ordinary, controlled);
                    count
                }
                Some(Glyph::Composite(composite)) => {
                    let plain: Vec<_> = composite.components().collect();
                    let mut cursor = composite.try_components();
                    let mut controlled = Vec::new();
                    while let Some(c) = cursor.try_next(&mut cx).unwrap() {
                        controlled.push(c);
                    }
                    assert_eq!(plain, controlled);
                    assert_eq!(
                        composite.count_and_instructions(),
                        composite.try_count_and_instructions(&mut cx).unwrap()
                    );
                    plain.len()
                }
                None => 0,
            };
            if let Some(gvar) = gvar.as_ref() {
                let data = gvar.glyph_variation_data(gid).unwrap();
                let checked = gvar
                    .try_glyph_variation_data(gid, &mut cx)
                    .unwrap()
                    .unwrap();
                assert_eq!(data.is_some(), checked.is_some());
                if let Some(data) = data {
                    let mut plain = data.tuples();
                    let mut checked = checked.unwrap().try_tuples();
                    while let Some(tuple) = plain.next() {
                        let typed = checked.try_next(&mut cx).unwrap().unwrap();
                        varied += 1;
                        for value in [
                            types::F2Dot14::NEG_ONE,
                            types::F2Dot14::ZERO,
                            types::F2Dot14::ONE,
                        ] {
                            let coords = vec![value; gvar.axis_count() as usize];
                            assert_eq!(
                                tuple.compute_scalar(&coords),
                                typed.try_compute_scalar(&coords, &mut cx).unwrap()
                            );
                            assert_eq!(
                                tuple.compute_scalar_f32(&coords),
                                typed.try_compute_scalar_f32(&coords, &mut cx).unwrap()
                            );
                        }
                        let ordinary: Vec<_> = tuple.deltas().collect();
                        let mut deltas = typed.try_deltas(&mut cx).unwrap();
                        let mut controlled = Vec::new();
                        while let Some(d) = deltas.try_next(&mut cx).unwrap() {
                            controlled.push(d);
                        }
                        assert_eq!(ordinary, controlled);
                        let mut plain_deltas =
                            vec![types::Point::<types::Fixed>::default(); point_count + 4];
                        let mut typed_deltas = plain_deltas.clone();
                        let mut plain_flags = vec![PointFlags::default(); point_count + 4];
                        let mut typed_flags = plain_flags.clone();
                        if tuple.has_deltas_for_all_points() {
                            let p =
                                tuple.accumulate_dense_deltas(&mut plain_deltas, types::Fixed::ONE);
                            let t = typed
                                .try_accumulate_dense_deltas(
                                    &mut typed_deltas,
                                    types::Fixed::ONE,
                                    &mut cx,
                                )
                                .unwrap();
                            assert_eq!(p, t);
                        } else {
                            let p = tuple.accumulate_sparse_deltas(
                                &mut plain_deltas,
                                &mut plain_flags,
                                types::Fixed::ONE,
                            );
                            let t = typed
                                .try_accumulate_sparse_deltas(
                                    &mut typed_deltas,
                                    &mut typed_flags,
                                    types::Fixed::ONE,
                                    &mut cx,
                                )
                                .unwrap();
                            assert_eq!(p, t);
                        }
                        assert_eq!(plain_deltas, typed_deltas);
                        assert_eq!(plain_flags, typed_flags);
                    }
                    assert!(checked.try_next(&mut cx).unwrap().is_none());
                }
                let coords = vec![types::F2Dot14::ONE; gvar.axis_count() as usize];
                let plain = gvar.phantom_point_deltas(&glyf, &loca, &coords, gid);
                let (r, log) = resources();
                let typed = gvar
                    .try_phantom_point_deltas(
                        &glyf,
                        &loca,
                        &coords,
                        gid,
                        &mut markdown_svg_budget::RequestControl(&r),
                    )
                    .unwrap();
                assert_eq!(plain, typed);
                assert_retired(&r, &log);
            }
        }
    }
    assert!(varied > 0);
}
#[test]
fn packed_zero_runs_count_skip_and_point_prescan_are_controlled_before_second_window() {
    use crate::tables::variations::{PackedDeltas, PackedPointNumbers};
    let bytes = vec![0xBF; 3000];
    let packed = PackedDeltas::consume_all(bytes.as_slice().into());
    let mut baseline = Steps::default();
    assert_eq!(
        packed.try_count_or_compute(&mut baseline).unwrap(),
        3000 * 64
    );
    assert_eq!(packed.count_or_compute(), 3000 * 64);
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(packed.try_count_or_compute(&mut fail), Err(at));
    assert_eq!(fail.work, &baseline.work[..at]);
    let mut baseline = Steps::default();
    packed
        .fetcher()
        .try_skip(3000 * 64, &mut baseline)
        .unwrap()
        .unwrap();
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(packed.fetcher().try_skip(3000 * 64, &mut fail), Err(at));
    assert_eq!(fail.work, &baseline.work[..at]);
    let mut encoded = (3000u16 | 0x8000).to_be_bytes().to_vec();
    for _ in 0..3000 {
        encoded.extend([0, 0]);
    }
    encoded.extend([1, 2]);
    let mut baseline = Steps::default();
    let (_, rest) =
        PackedPointNumbers::try_split_off_front(encoded.as_slice().into(), &mut baseline).unwrap();
    assert_eq!(rest.as_bytes(), &[1, 2]);
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(256)
        .unwrap()
        .0
        + 1;
    let mut fail = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(
        PackedPointNumbers::try_split_off_front(encoded.as_slice().into(), &mut fail).err(),
        Some(at)
    );
    assert_eq!(fail.work, &baseline.work[..at]);
}

#[test]
fn checked_provider_preserves_top_tables_and_rejects_foreign_request_before_lookup() {
    let bytes = font_test_data::VAZIRMATN_VAR;
    let ordinary = FontRef::new(bytes).unwrap();
    let (r, log) = resources();
    let font = crate::request::CheckedFontRef::from_index(bytes, 0, &r)
        .unwrap()
        .unwrap();
    let mut cx = markdown_svg_budget::RequestControl(&r);
    assert_eq!(
        font.try_head(&mut cx).unwrap().unwrap().units_per_em(),
        ordinary.head().unwrap().units_per_em()
    );
    assert_eq!(
        font.try_hmtx(&mut cx)
            .unwrap()
            .unwrap()
            .advance(types::GlyphId::new(1)),
        ordinary.hmtx().unwrap().advance(types::GlyphId::new(1))
    );
    assert_eq!(
        font.try_loca(None, &mut cx).unwrap().unwrap().len(),
        ordinary.loca(None).unwrap().len()
    );
    let (other, other_log) = resources();
    let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
    assert_eq!(
        font.try_head(&mut markdown_svg_budget::RequestControl(&other))
            .err(),
        Some(invariant)
    );
    assert_eq!(other.failure(), Some(invariant));
    assert_eq!(r.failure(), None);
    assert_eq!(other_log.lock().unwrap().count(Call::Work), 0);
    assert_retired(&r, &log);
}

fn device_metrics_bytes(records: u16) -> Vec<u8> {
    let mut bytes = vec![0, 0];
    bytes.extend(records.to_be_bytes());
    bytes.extend(4u32.to_be_bytes());
    for index in 0..records {
        bytes.extend([index.min(255) as u8, 255, index as u8, (index >> 8) as u8]);
    }
    bytes
}

#[test]
fn hdmx_checked_records_search_and_borrowed_widths_match_ordinary() {
    use tables::hdmx::Hdmx;
    let bytes = device_metrics_bytes(300);
    let table = Hdmx::read(bytes.as_slice().into(), 2).unwrap();
    let mut cx = Steps::default();
    let checked = Hdmx::try_read_with_args(bytes.as_slice().into(), &2, &mut cx)
        .unwrap()
        .unwrap();
    let records = checked.try_records(&mut cx).unwrap();
    let mut cursor = records.try_iter();
    for ordinary in table.records().iter() {
        let ordinary = ordinary.unwrap();
        let actual = cursor.try_next(&mut cx).unwrap().unwrap().unwrap();
        assert_eq!(actual, ordinary);
        assert_eq!(actual.widths.as_ptr(), ordinary.widths.as_ptr());
    }
    assert!(cursor.try_next(&mut cx).unwrap().is_none());
    for size in 0..=u8::MAX {
        assert_eq!(
            checked.try_record_for_size(size, &mut cx).unwrap(),
            table.record_for_size(size)
        );
    }
    // Broken record storage keeps the old empty-array fallback; a malformed
    // record itself remains a font error, rather than an outer control error.
    let short = &bytes[..bytes.len() - 1];
    let short = Hdmx::read(short.into(), 2).unwrap();
    assert_eq!(
        short.try_records(&mut cx).unwrap().len(),
        short.records().len()
    );
    assert!(short.try_record_for_size(4, &mut cx).unwrap().is_none());
    let mut empty_record_bytes = device_metrics_bytes(1);
    empty_record_bytes[4..8].copy_from_slice(&0u32.to_be_bytes());
    let empty = Hdmx::read(empty_record_bytes.as_slice().into(), 2).unwrap();
    assert!(empty.try_records(&mut cx).unwrap().is_empty());
}

#[test]
fn hdmx_search_stops_before_second_comparison() {
    let bytes = device_metrics_bytes(200);
    let table = tables::hdmx::Hdmx::read(bytes.as_slice().into(), 2).unwrap();
    let mut baseline = Steps::default();
    assert!(table
        .try_record_for_size(0, &mut baseline)
        .unwrap()
        .is_some());
    let at = baseline
        .work
        .iter()
        .enumerate()
        .filter(|e| *e.1 == 1)
        .nth(1)
        .unwrap()
        .0
        + 1;
    let mut stop = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(table.try_record_for_size(0, &mut stop), Err(at));
    assert_eq!(stop.work, baseline.work[..at]);
}

#[test]
fn hdmx_cursor_second_256_window_keeps_request_error_and_never_advances() {
    let bytes = device_metrics_bytes(512);
    let table = tables::hdmx::Hdmx::read(bytes.as_slice().into(), 2).unwrap();
    let (r, log) = resources();
    let mut cx = markdown_svg_budget::RequestControl(&r);
    let records = table.try_records(&mut cx).unwrap();
    let mut cursor = records.try_iter();
    for _ in 0..256 {
        cursor.try_next(&mut cx).unwrap().unwrap().unwrap();
    }
    // The very next cursor gate precedes the next record parse.
    let next = log.lock().unwrap().count(Call::Work) + 1;
    log.lock().unwrap().fault = Some((Call::Work, next, failure()));
    assert_eq!(cursor.try_next(&mut cx).err(), Some(failure()));
    assert_eq!(r.failure(), Some(failure()));
    let calls = log.lock().unwrap().events.len();
    assert_eq!(cursor.try_next(&mut cx).err(), Some(failure()));
    assert_eq!(table.try_record_for_size(0, &mut cx).err(), Some(failure()));
    assert_eq!(log.lock().unwrap().events.len(), calls);
    assert_retired(&r, &log);
}

#[test]
fn layout_typed_lists_and_script_selection_match_real_fonts() {
    let mut seen_scripts = 0;
    for bytes in [
        font_test_data::NOTOSERIFHEBREW_AUTOHINT_METRICS,
        font_test_data::NOTOSERIF_AUTOHINT_SHAPING,
        font_test_data::CANTARELL_VF_TRIMMED,
        font_test_data::MATERIAL_SYMBOLS_SUBSET,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let mut cx = Steps::default();
        let gsub = font.gsub().unwrap();
        let scripts = gsub.try_script_list(&mut cx).unwrap().unwrap();
        let features = gsub.try_feature_list(&mut cx).unwrap().unwrap();
        let lookups = gsub.try_lookup_list(&mut cx).unwrap().unwrap();
        assert_eq!(
            lookups.lookup_offsets(),
            gsub.lookup_list().unwrap().lookup_offsets()
        );
        for tag in [
            types::Tag::new(b"hebr"),
            types::Tag::new(b"bng3"),
            types::Tag::new(b"TRK "),
        ] {
            assert_eq!(
                scripts.try_select(&[tag], &mut cx).unwrap(),
                scripts.select(&[tag])
            );
        }
        for index in 0..scripts.script_count() {
            let ordinary = scripts.get(index).unwrap();
            let actual = scripts.try_get(index, &mut cx).unwrap().unwrap();
            seen_scripts += 1;
            assert_eq!(actual.tag, ordinary.tag);
            assert_eq!(
                actual.lang_sys_records().len(),
                ordinary.lang_sys_records().len()
            );
            assert_eq!(
                actual.lang_sys_records().as_ptr(),
                ordinary.lang_sys_records().as_ptr()
            );
            for tag in [types::Tag::new(b"TRK "), types::Tag::new(b"FRA ")] {
                assert_eq!(
                    actual.try_lang_sys_index_for_tag(tag, &mut cx).unwrap(),
                    ordinary.lang_sys_index_for_tag(tag)
                );
            }
            for index in 0..actual.lang_sys_count() {
                let typed = actual.try_lang_sys(index, &mut cx).unwrap().unwrap();
                assert_eq!(typed.tag, ordinary.lang_sys(index).unwrap().tag);
            }
            if let Some(Ok(language)) = actual.try_default_lang_sys(&mut cx).unwrap() {
                for tag in [
                    types::Tag::new(b"rlig"),
                    types::Tag::new(b"liga"),
                    types::Tag::new(b"rclt"),
                ] {
                    assert_eq!(
                        language
                            .try_feature_index_for_tag(&features, tag, &mut cx)
                            .unwrap(),
                        language.feature_index_for_tag(&features, tag)
                    );
                }
            }
        }
        for index in 0..features.feature_count() {
            let actual = features.try_get(index, &mut cx).unwrap().unwrap();
            let ordinary = features.get(index).unwrap();
            assert_eq!(actual.tag, ordinary.tag);
            assert_eq!(actual.lookup_list_indices(), ordinary.lookup_list_indices());
        }
        if let Ok(gpos) = font.gpos() {
            assert_eq!(
                gpos.try_script_list(&mut cx)
                    .unwrap()
                    .unwrap()
                    .script_count(),
                gpos.script_list().unwrap().script_count()
            );
            assert_eq!(
                gpos.try_feature_list(&mut cx)
                    .unwrap()
                    .unwrap()
                    .feature_count(),
                gpos.feature_list().unwrap().feature_count()
            );
            assert_eq!(
                gpos.try_lookup_list(&mut cx)
                    .unwrap()
                    .unwrap()
                    .lookup_offsets(),
                gpos.lookup_list().unwrap().lookup_offsets()
            );
        }
    }
    assert!(seen_scripts > 5);
}

#[test]
fn layout_feature_variation_nullable_font_error_and_control_remain_distinct() {
    let mut bytes = vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut cx = Steps::default();
    assert!(tables::gsub::Gsub::read(bytes.as_slice().into())
        .unwrap()
        .try_feature_variations(&mut cx)
        .unwrap()
        .is_none());
    bytes[3] = 1;
    bytes.extend(0u32.to_be_bytes());
    let null = tables::gsub::Gsub::read(bytes.as_slice().into()).unwrap();
    assert!(null.try_feature_variations(&mut cx).unwrap().is_none());
    bytes[10..14].copy_from_slice(&1000u32.to_be_bytes());
    let bad = tables::gsub::Gsub::read(bytes.as_slice().into()).unwrap();
    assert!(matches!(
        bad.try_feature_variations(&mut cx),
        Ok(Some(Err(ReadError::OutOfBounds)))
    ));
    let mut stop = Steps {
        stop: Some(1),
        ..Default::default()
    };
    assert_eq!(bad.try_feature_variations(&mut stop).err(), Some(1));
    assert_eq!(stop.work.len(), 1);
}

#[test]
fn aat_morx_and_kerx_long_prefix_cancel_before_second_256_window_yield() {
    use tables::{kerx::Kerx, morx::Morx};
    for is_morx in [true, false] {
        let length = if is_morx { 16u32 } else { 12u32 };
        let mut bytes = vec![0, 2, 0, 0];
        bytes.extend(600u32.to_be_bytes());
        for _ in 0..600 {
            if is_morx {
                bytes.extend(0u32.to_be_bytes());
            }
            bytes.extend(length.to_be_bytes());
            bytes.extend([0; 8]);
        }
        let mut baseline = Steps::default();
        if is_morx {
            let table = Morx::read(bytes.as_slice().into()).unwrap();
            let actual = table.try_chains(&mut baseline).unwrap();
            assert_eq!(actual.iter().count(), table.chains().iter().count());
            assert_eq!(actual.iter().count(), 600);
        } else {
            let table = Kerx::read(bytes.as_slice().into()).unwrap();
            let actual = table.try_subtables(&mut baseline).unwrap();
            assert_eq!(actual.iter().count(), table.subtables().iter().count());
            assert_eq!(actual.iter().count(), 600);
        }
        let at = baseline
            .work
            .iter()
            .enumerate()
            .filter(|e| *e.1 == 1)
            .nth(256)
            .unwrap()
            .0
            + 1;
        let (r, log) = resources();
        log.lock().unwrap().fault = Some((Call::Work, at, failure()));
        let mut cx = markdown_svg_budget::RequestControl(&r);
        let actual = if is_morx {
            Morx::read(bytes.as_slice().into())
                .unwrap()
                .try_chains(&mut cx)
                .err()
        } else {
            Kerx::read(bytes.as_slice().into())
                .unwrap()
                .try_subtables(&mut cx)
                .err()
        };
        assert_eq!(actual, Some(failure()));
        assert_eq!(r.failure(), Some(failure()));
        assert_eq!(log.lock().unwrap().events.len(), at);
        assert_retired(&r, &log);
    }
}

#[test]
fn aat_checked_lookup_keeps_original_values_and_control_before_binary_search() {
    use tables::aat::LookupU16;
    // Format 2: three fixed segments, mapping 20..22, 23..24, 25..28.
    let words = [2u16, 6, 3, 12, 1, 6, 22, 20, 4, 24, 23, 5, 28, 25, 6];
    let bytes: Vec<_> = words.into_iter().flat_map(u16::to_be_bytes).collect();
    let mut cx = Steps::default();
    let typed = LookupU16::try_read(bytes.as_slice().into(), &mut cx)
        .unwrap()
        .unwrap();
    for glyph in [0, 19, 20, 22, 23, 24, 25, 28, 29, u16::MAX] {
        assert_eq!(typed.try_value(glyph, &mut cx).unwrap(), typed.value(glyph));
    }
    let mut baseline = Steps::default();
    assert_eq!(typed.try_value(23, &mut baseline).unwrap(), Ok(5));
    let at = baseline.work.iter().position(|&work| work > 0).unwrap() + 1;
    let mut stop = Steps {
        stop: Some(at),
        ..Default::default()
    };
    assert_eq!(typed.try_value(23, &mut stop), Err(at));
    assert_eq!(stop.work, baseline.work[..at]);
}

#[test]
fn real_morx_typed_kinds_state_and_lookup_values_match_all_five_formats() {
    use tables::morx::{Morx, SubtableKind};
    let mut seen = 0u32;
    for bytes in [
        font_test_data::morx::ONE,
        font_test_data::morx::FOUR,
        font_test_data::morx::EIGHTEEN,
        font_test_data::morx::THIRTY_FOUR,
        font_test_data::morx::FORTY_ONE,
    ] {
        let (r, log) = resources();
        let font = request::CheckedFontRef::from_index(bytes, 0, &r)
            .unwrap()
            .unwrap();
        let mut cx = markdown_svg_budget::RequestControl(&r);
        let table = font.try_expect_table::<Morx>(&mut cx).unwrap().unwrap();
        let array = table.try_chains(&mut cx).unwrap();
        let mut chains = array.try_iter();
        while let Some(chain) = chains.try_next(&mut cx).unwrap() {
            let chain = chain.unwrap();
            let array = chain.try_subtables(&mut cx).unwrap();
            let mut subtables = array.try_iter();
            while let Some(subtable) = subtables.try_next(&mut cx).unwrap() {
                let subtable = subtable.unwrap();
                seen |= 1 << (subtable.coverage() & 0xFF);
                let typed = subtable.try_kind(&mut cx).unwrap().unwrap();
                let ordinary = subtable.kind().unwrap();
                match (typed, ordinary) {
                    (SubtableKind::NonContextual(a), SubtableKind::NonContextual(b)) => {
                        for id in 0..16 {
                            assert_eq!(a.try_value(id, &mut cx).unwrap(), b.value(id));
                        }
                    }
                    (SubtableKind::Rearrangement(a), SubtableKind::Rearrangement(b)) => {
                        for id in 0..16 {
                            assert_eq!(
                                a.try_class(types::GlyphId::new(id), &mut cx).unwrap(),
                                b.class(types::GlyphId::new(id))
                            );
                        }
                        for state in 0..3 {
                            for class in 0..a.n_classes as u16 {
                                let a = a.try_entry(state, class, &mut cx).unwrap();
                                let b = b.entry(state, class);
                                assert_eq!(
                                    a.as_ref().map(|e| (e.new_state, e.flags)),
                                    b.as_ref().map(|e| (e.new_state, e.flags))
                                );
                            }
                        }
                    }
                    (SubtableKind::Contextual(a), SubtableKind::Contextual(b)) => {
                        for index in 0..a.lookups.len() {
                            // The format has no explicit offset-array length;
                            // the original view extends to the end of the table.
                            // Compare every index, including original font errors.
                            match (
                                a.lookups.try_get(index, &mut cx).unwrap(),
                                b.lookups.get(index),
                            ) {
                                (Ok(a), Ok(b)) => {
                                    for id in 0..16 {
                                        assert_eq!(a.try_value(id, &mut cx).unwrap(), b.value(id));
                                    }
                                }
                                (Err(a), Err(b)) => assert_eq!(a, b),
                                _ => panic!("typed contextual lookup changed a font error"),
                            }
                        }
                    }
                    (SubtableKind::Ligature(a), SubtableKind::Ligature(b)) => {
                        assert_eq!(a.ligature_actions, b.ligature_actions);
                        assert_eq!(a.components, b.components);
                        assert_eq!(a.ligatures, b.ligatures);
                    }
                    (SubtableKind::Insertion(a), SubtableKind::Insertion(b)) => {
                        assert_eq!(a.glyphs, b.glyphs);
                    }
                    _ => panic!("ordinary and typed morx selected different formats"),
                }
            }
        }
        assert_eq!(r.failure(), None);
        assert_retired(&r, &log);
    }
    assert_eq!(seen, (1 << 0) | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5));
}
#[test]
fn actual_varc_components_indices_and_sparse_regions_match_controlled() {
    let mut components = 0;
    for bytes in [
        font_test_data::varc::CJK_6868,
        font_test_data::varc::CONDITIONALS,
    ] {
        let font = FontRef::new(bytes).unwrap();
        let varc = font.varc().unwrap();
        let mut cx = Steps::default();
        let indices = varc.try_var_composite_glyphs(&mut cx).unwrap().unwrap();
        for id in 0..indices.count() {
            let glyph = varc.glyph(id as usize).unwrap();
            let typed = varc.try_glyph(id as usize, &mut cx).unwrap().unwrap();
            let mut ordinary = glyph.components();
            let mut controlled = typed.try_components();
            while let Some(component) = ordinary.next() {
                let component = component.unwrap();
                let typed = controlled.try_next(&mut cx).unwrap().unwrap().unwrap();
                components += 1;
                assert_eq!(component.flags(), typed.flags());
                assert_eq!(component.gid(), typed.gid());
                assert_eq!(component.condition_index(), typed.condition_index());
                assert_eq!(component.axis_indices_index(), typed.axis_indices_index());
                assert_eq!(
                    component.axis_values_var_index(),
                    typed.axis_values_var_index()
                );
                assert_eq!(component.transform_var_index(), typed.transform_var_index());
                assert_eq!(
                    component.transform().matrix().elements().map(f32::to_bits),
                    typed.transform().matrix().elements().map(f32::to_bits)
                );
                if let Some(values) = component.axis_values() {
                    let ordinary: Vec<_> = values.iter().collect();
                    let mut cursor = typed.axis_values().unwrap().try_iter(&mut cx).unwrap();
                    let mut actual = Vec::new();
                    while let Some(v) = cursor.try_next(&mut cx).unwrap() {
                        actual.push(v)
                    }
                    assert_eq!(ordinary, actual);
                }
            }
            assert!(controlled.try_next(&mut cx).unwrap().is_none());
        }
        if let Some(Ok(store)) = varc.try_multi_var_store(&mut cx).unwrap() {
            let regions = store.try_region_list(&mut cx).unwrap().unwrap();
            let mut cursor = regions.regions().try_iter();
            while let Some(region) = cursor.try_next(&mut cx).unwrap() {
                let region = region.unwrap();
                for value in [types::F2Dot14::ZERO, types::F2Dot14::ONE] {
                    let coords = [value; 8];
                    assert_eq!(
                        region.compute_scalar_f32(&coords).to_bits(),
                        region
                            .try_compute_scalar_f32(&coords, &mut cx)
                            .unwrap()
                            .to_bits()
                    );
                }
            }
        }
    }
    assert!(components > 0);
}
#[test]
fn huge_cff2_index_stays_font_error_and_original_control_error_wins() {
    let bytes = [255, 255, 255, 255, 4];
    let index = crate::ps::cff::index::Index2::read(bytes.as_slice().into()).unwrap();
    let mut cx = Steps::default();
    assert!(index.try_get(0, &mut cx).unwrap().is_err());
    assert!(index.try_size_in_bytes(&mut cx).unwrap().is_err());
    let mut stop = Steps {
        stop: Some(1),
        ..Default::default()
    };
    assert_eq!(index.try_get(0, &mut stop).err(), Some(1));
}

#[test]
fn charstring_declared_depth_termination_is_typed_work_only_in_request_mode() {
    let local_data = [0u8, 1, 1, 1, 3, 32, 10];
    let local = Index::new(&local_data, false).unwrap();
    let empty = Index::Empty;
    let bytes = [32u8, 10];
    let mut ordinary = Commands::default();
    let plain = cs::evaluate(
        &(&[] as &[u8], &empty, &empty, &local),
        None,
        &bytes,
        &mut ordinary,
    );
    assert!(matches!(
        plain,
        Err(crate::ps::error::Error::CharstringNestingDepthLimitExceeded)
    ));
    let (r, log) = resources();
    let context = RequestCffContext::new(&[], &empty, &empty, &local, &r);
    let mut sink = Commands::default();
    let quota = Error::new(FailureKind::Quota, LimitKind::Work);
    assert_eq!(
        cs::try_evaluate(&context, None, &bytes, &mut sink).err(),
        Some(quota)
    );
    assert_eq!(r.failure(), Some(quota));
    assert!(sink.points.is_empty());
    assert_eq!(sink.finishes, 0);
    assert_retired(&r, &log);
}
#[test]
fn zero_point_simple_glyph_ignores_trailing_flags_without_indexing_empty_output() {
    use crate::tables::glyf::{PointFlags, SimpleGlyph};
    let mut bytes = vec![0u8; 12];
    bytes.extend([0x31, 0]);
    let glyph = SimpleGlyph::read(bytes.as_slice().into()).unwrap();
    assert_eq!(glyph.num_points(), 0);
    let mut points: [types::Point<i32>; 0] = [];
    let mut flags: [PointFlags; 0] = [];
    glyph
        .try_read_points_fast(&mut points, &mut flags, &mut Steps::default())
        .unwrap()
        .unwrap();
    glyph.read_points_fast(&mut points, &mut flags).unwrap();
}
