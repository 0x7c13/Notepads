// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Observation vectors and font bootstrap are test allocations, not RSS claims.

use super::*;
use crate::control::{test_support::*, FontControl, Ordinary};
use crate::instance::LocationRef;
use crate::request::{ColorGlyphs, ColorPalettes as CheckedPalettes};
use crate::MetadataProvider;
use markdown_svg_budget::{
    BudgetVec, Error, Limits, RequestControl, ResourceKind, ResourcesRef,
};
use read_fonts::{types::F2Dot14, TableProvider};
use std::sync::{Arc, Mutex};

const NORMAL: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-normal-v1.ttf");
const VARIABLE: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-var-v1.ttf");
const PAD: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-pad-v1.ttf");
const GRAPH: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-graph-v1.ttf");
const WIDTH_V0: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-width-v0.ttf");
const WIDTH_V1: &[u8] = include_bytes!("../../tests/fixtures/color-control-v1/colr-width-v1.ttf");
const OP_ERROR: Error = Error::native(-0x5E02);

fn matrix(t: Transform) -> [u32; 6] {
    [t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(f32::to_bits)
}
#[derive(Debug, PartialEq, Eq)]
enum BrushBits {
    Solid(u16, u32),
    Gradient(u8, Vec<u32>, Vec<(u32, u16, u32)>, String),
}
fn brush_bits(brush: Brush<'_>) -> BrushBits {
    let stops = |stops: &[ColorStop]| {
        stops
            .iter()
            .map(|s| (s.offset.to_bits(), s.palette_index, s.alpha.to_bits()))
            .collect()
    };
    match brush {
        Brush::Solid {
            palette_index,
            alpha,
        } => BrushBits::Solid(palette_index, alpha.to_bits()),
        Brush::LinearGradient {
            p0,
            p1,
            color_stops,
            extend,
        } => BrushBits::Gradient(
            0,
            [p0.x, p0.y, p1.x, p1.y].map(f32::to_bits).to_vec(),
            stops(color_stops),
            format!("{extend:?}"),
        ),
        Brush::RadialGradient {
            c0,
            r0,
            c1,
            r1,
            color_stops,
            extend,
        } => BrushBits::Gradient(
            1,
            [c0.x, c0.y, r0, c1.x, c1.y, r1].map(f32::to_bits).to_vec(),
            stops(color_stops),
            format!("{extend:?}"),
        ),
        Brush::SweepGradient {
            c0,
            start_angle,
            end_angle,
            color_stops,
            extend,
        } => BrushBits::Gradient(
            2,
            [c0.x, c0.y, start_angle, end_angle]
                .map(f32::to_bits)
                .to_vec(),
            stops(color_stops),
            format!("{extend:?}"),
        ),
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Op {
    Transform([u32; 6]),
    PopTransform,
    ClipGlyph(u32),
    ClipBox([u32; 4]),
    PopClip,
    Fill(BrushBits),
    FillGlyph(u32, Option<[u32; 6]>, BrushBits),
    Layer(String),
    PopLayer(String),
    Cache(u32),
}
#[derive(Clone, Copy, Debug)]
enum Cache {
    Miss,
    Hit,
    FontError,
}
impl Cache {
    fn result(self, glyph: GlyphId) -> Result<PaintCachedColorGlyph, PaintError> {
        match self {
            Self::Miss => Ok(PaintCachedColorGlyph::Unimplemented),
            Self::Hit => Ok(PaintCachedColorGlyph::Ok),
            Self::FontError => Err(PaintError::GlyphNotFound(glyph)),
        }
    }
}
struct Plain {
    ops: Vec<Op>,
    cache: Cache,
}
impl Plain {
    fn new(cache: Cache) -> Self {
        Self {
            ops: Vec::new(),
            cache,
        }
    }
}
impl ColorPainter for Plain {
    fn push_transform(&mut self, t: Transform) {
        self.ops.push(Op::Transform(matrix(t)));
    }
    fn pop_transform(&mut self) {
        self.ops.push(Op::PopTransform);
    }
    fn push_clip_glyph(&mut self, g: GlyphId) {
        self.ops.push(Op::ClipGlyph(g.to_u32()));
    }
    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        self.ops.push(Op::ClipBox(
            [b.x_min, b.y_min, b.x_max, b.y_max].map(f32::to_bits),
        ));
    }
    fn pop_clip(&mut self) {
        self.ops.push(Op::PopClip);
    }
    fn fill(&mut self, b: Brush<'_>) {
        self.ops.push(Op::Fill(brush_bits(b)));
    }
    fn fill_glyph(&mut self, g: GlyphId, t: Option<Transform>, b: Brush<'_>) {
        self.ops
            .push(Op::FillGlyph(g.to_u32(), t.map(matrix), brush_bits(b)));
    }
    fn push_layer(&mut self, m: CompositeMode) {
        self.ops.push(Op::Layer(format!("{m:?}")));
    }
    fn pop_layer_with_mode(&mut self, m: CompositeMode) {
        self.ops.push(Op::PopLayer(format!("{m:?}")));
    }
    fn paint_cached_color_glyph(
        &mut self,
        g: GlyphId,
    ) -> Result<PaintCachedColorGlyph, PaintError> {
        self.ops.push(Op::Cache(g.to_u32()));
        self.cache.result(g)
    }
}
struct Checked<'r> {
    r: &'r ResourcesRef,
    events: Arc<Mutex<Events>>,
    ops: Vec<Op>,
    cache: Cache,
    aborts: usize,
    after_failure: usize,
    first_callback_work: Option<usize>,
    callback_work: Vec<usize>,
    fault_at: Option<usize>,
    fault_returns_ok: bool,
    temporary_on_event: bool,
    temporary_on_cache: bool,
    temporary_releases: Vec<usize>,
}
impl<'r> Checked<'r> {
    fn new(r: &'r ResourcesRef, events: &Arc<Mutex<Events>>, cache: Cache) -> Self {
        Self {
            r,
            events: events.clone(),
            ops: Vec::new(),
            cache,
            aborts: 0,
            after_failure: 0,
            first_callback_work: None,
            callback_work: Vec::new(),
            fault_at: None,
            fault_returns_ok: false,
            temporary_on_event: false,
            temporary_on_cache: false,
            temporary_releases: Vec::new(),
        }
    }
    fn temporary(&mut self) -> Result<BudgetVec<u8>, Error> {
        let mut value = BudgetVec::new(self.r.clone(), ResourceKind::Scratch);
        value.try_push(7)?;
        self.temporary_releases
            .push(self.events.lock().unwrap().release + 1);
        Ok(value)
    }
    fn event(&mut self, event: Op) -> Result<(), Error> {
        if self.r.failure().is_some() {
            self.after_failure += 1;
        }
        let work = self.events.lock().unwrap().positive_work;
        self.first_callback_work.get_or_insert(work);
        self.callback_work.push(work);
        self.ops.push(event);
        let _natural_temporary = if self.temporary_on_event {
            Some(self.temporary()?)
        } else {
            None
        };
        if self.fault_at == Some(self.ops.len()) {
            let error = self.r.record(OP_ERROR);
            if !self.fault_returns_ok {
                return Err(error);
            }
        }
        Ok(())
    }
}
impl crate::request::ColorPainter for Checked<'_> {
    fn resources(&self) -> &ResourcesRef {
        &self.r
    }
    fn push_transform(&mut self, t: Transform) -> Result<(), Error> {
        self.event(Op::Transform(matrix(t)))
    }
    fn pop_transform(&mut self) -> Result<(), Error> {
        self.event(Op::PopTransform)
    }
    fn push_clip_glyph(&mut self, g: GlyphId) -> Result<(), Error> {
        self.event(Op::ClipGlyph(g.to_u32()))
    }
    fn push_clip_box(&mut self, b: BoundingBox<f32>) -> Result<(), Error> {
        self.event(Op::ClipBox(
            [b.x_min, b.y_min, b.x_max, b.y_max].map(f32::to_bits),
        ))
    }
    fn pop_clip(&mut self) -> Result<(), Error> {
        self.event(Op::PopClip)
    }
    fn fill(&mut self, b: Brush<'_>) -> Result<(), Error> {
        self.event(Op::Fill(brush_bits(b)))
    }
    fn fill_glyph(&mut self, g: GlyphId, t: Option<Transform>, b: Brush<'_>) -> Result<(), Error> {
        self.event(Op::FillGlyph(g.to_u32(), t.map(matrix), brush_bits(b)))
    }
    fn push_layer(&mut self, m: CompositeMode) -> Result<(), Error> {
        self.event(Op::Layer(format!("{m:?}")))
    }
    fn pop_layer_with_mode(&mut self, m: CompositeMode) -> Result<(), Error> {
        self.event(Op::PopLayer(format!("{m:?}")))
    }
    fn paint_cached_color_glyph(
        &mut self,
        g: GlyphId,
    ) -> Result<Result<PaintCachedColorGlyph, PaintError>, Error> {
        self.event(Op::Cache(g.to_u32()))?;
        let _natural_temporary = if self.temporary_on_cache {
            Some(self.temporary()?)
        } else {
            None
        };
        Ok(self.cache.result(g))
    }
    fn abort_color_glyph(&mut self) {
        self.aborts += 1;
    }
}
fn plain(
    data: &[u8],
    id: u32,
    coords: &[F2Dot14],
    cache: Cache,
) -> (Result<(), PaintError>, Plain) {
    let font = FontRef::new(data).unwrap();
    let mut p = Plain::new(cache);
    let result = font
        .color_glyphs()
        .get(GlyphId::new(id))
        .unwrap()
        .paint(LocationRef::new(coords), &mut p);
    (result, p)
}
fn checked(
    data: &[u8],
    id: u32,
    coords: &[F2Dot14],
    r: &ResourcesRef,
    p: &mut Checked,
) -> Result<Result<(), PaintError>, Error> {
    let font = FontRef::try_new(data, &mut RequestControl(r))?.unwrap();
    let glyphs = ColorGlyphs::try_new(&font, r)?;
    let glyph = glyphs.try_get(GlyphId::new(id))?.unwrap();
    glyph.paint(LocationRef::new(coords), p)
}
fn retired(r: &ResourcesRef, events: &Arc<Mutex<Events>>, fault: Option<Fault>) {
    let failed = events.lock().unwrap().failed_release_bytes as u64;
    let failed_release = u64::from(matches!(fault, Some(Fault::Release(_))));
    assert_eq!(r.statistics().live_leases, failed_release);
    assert_eq!(r.statistics().scratch_bytes, failed);
    assert_eq!(r.statistics().release_failures, failed_release);
    if failed_release != 0 {
        assert_eq!(r.validate_retired(), Err(FAULT));
    } else {
        r.validate_retired().unwrap();
    }
}

#[test]
fn real_encoded_normal_variable_all_gradient_boundaries_match_complete_callback_bits() {
    for data in [NORMAL, VARIABLE] {
        for coordinate in [0.0, 0.5, 1.0] {
            let coords = [F2Dot14::from_f32(coordinate)];
            for id in 2..20 {
                let (a, p) = plain(data, id, &coords, Cache::Miss);
                a.unwrap();
                let (r, e) = resources(None, Limits::default());
                let mut q = Checked::new(&r, &e, Cache::Miss);
                checked(data, id, &coords, &r, &mut q).unwrap().unwrap();
                assert_eq!(p.ops, q.ops, "id={id} coord={coordinate}");
                assert_eq!(q.aborts, 0);
                assert_eq!(q.after_failure, 0);
                retired(&r, &e, None);
            }
        }
    }
    let (_, a) = plain(VARIABLE, 2, &[F2Dot14::ZERO], Cache::Miss);
    let (_, b) = plain(VARIABLE, 2, &[F2Dot14::from_f32(0.5)], Cache::Miss);
    assert_ne!(
        a.ops, b.ops,
        "real nonzero variation store must affect callbacks"
    );
    println!("COLOR real encoded normal/variable 2x3x18=108 complete bitwise callback comparisons");
}

#[test]
fn original_fontations_every_color_glyph_and_clipbox_match_checked_at_three_locations() {
    let mut comparisons = 0;
    for data in [font_test_data::COLRV0V1, font_test_data::COLRV0V1_VARIABLE] {
        let font = FontRef::new(data).unwrap();
        let axis_count = font.axes().len();
        let count = font.maxp().unwrap().num_glyphs() as u32;
        let original = font.color_glyphs();
        let (r, e) = resources(None, Limits::default());
        let collection = ColorGlyphs::try_new(&font, &r).unwrap();
        for coordinate in [0.0, 0.5, 1.0] {
            let coords = vec![F2Dot14::from_f32(coordinate); axis_count];
            for id in 0..count {
                let a = original.get(GlyphId::new(id));
                let b = collection.try_get(GlyphId::new(id)).unwrap();
                assert_eq!(a.is_some(), b.is_some());
                let (Some(a), Some(b)) = (a, b) else {
                    continue;
                };
                let bits = |value: Option<BoundingBox<f32>>| {
                    value.map(|b| [b.x_min, b.y_min, b.x_max, b.y_max].map(f32::to_bits))
                };
                assert_eq!(
                    bits(a.bounding_box(LocationRef::new(&coords), Size::new(17.5))),
                    bits(
                        b.try_bounding_box(LocationRef::new(&coords), Size::new(17.5))
                            .unwrap()
                    )
                );
                let mut p = Plain::new(Cache::Miss);
                let mut q = Checked::new(&r, &e, Cache::Miss);
                let a = a.paint(LocationRef::new(&coords), &mut p);
                let b = b.paint(LocationRef::new(&coords), &mut q).unwrap();
                assert_eq!(
                    format!("{a:?}"),
                    format!("{b:?}"),
                    "glyph {id} coord {coordinate}"
                );
                assert_eq!(p.ops, q.ops, "glyph {id} coord {coordinate}");
                comparisons += 1;
            }
        }
        retired(&r, &e, None);
    }
    assert!(comparisons >= 1000);
    println!("COLOR original fontations colored glyphs three locations complete bits/clipbox comparisons={comparisons}");
}

#[test]
fn real_pad_32_and_256_insert_paid_extra_stop_and_keep_repeat_reflect_empty() {
    for id in 2..20 {
        let (a, p) = plain(PAD, id, &[], Cache::Miss);
        a.unwrap();
        let (r, e) = resources(None, Limits::default());
        let mut q = Checked::new(&r, &e, Cache::Miss);
        checked(PAD, id, &[], &r, &mut q).unwrap().unwrap();
        assert_eq!(p.ops, q.ops, "pad glyph {id}");
        let relative = id - 2;
        let extend = relative % 3;
        let n = if relative % 6 < 3 { 32 } else { 256 };
        let lengths = q
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::FillGlyph(_, _, BrushBits::Gradient(_, _, s, _)) => Some(s.len()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if extend == 0 {
            assert_eq!(lengths, [n as usize + 1]);
            assert!(e.lock().unwrap().reserve > 0);
        } else {
            assert!(lengths.is_empty());
        }
        retired(&r, &e, None);
    }
}

#[test]
fn actual_cache_optimization_cycle_depth_and_font_error_keep_original_sequences() {
    for id in 2..9 {
        for cache in [Cache::Miss, Cache::Hit, Cache::FontError] {
            let (a, p) = plain(GRAPH, id, &[], cache);
            let (r, e) = resources(None, Limits::default());
            let mut q = Checked::new(&r, &e, cache);
            let b = checked(GRAPH, id, &[], &r, &mut q).unwrap();
            assert_eq!(
                format!("{a:?}"),
                format!("{b:?}"),
                "id={id} cache={cache:?}"
            );
            assert_eq!(p.ops, q.ops, "id={id} cache={cache:?}");
            assert_eq!(q.aborts, usize::from(b.is_err()));
            retired(&r, &e, None);
        }
    }
    let (_, p) = plain(GRAPH, 5, &[], Cache::Miss);
    assert!(
        matches!(p.ops.first(), Some(Op::ClipGlyph(_))),
        "fallback must really traverse twice"
    );
    assert!(p.ops.iter().any(|e| matches!(e, Op::Layer(_))));
}

#[test]
fn all_real_stop_heap_sort_reserve_commit_release_failures_stop_candidate() {
    let coords = [F2Dot14::from_f32(0.5)];
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    checked(VARIABLE, 19, &coords, &r, &mut p).unwrap().unwrap();
    let counts = {
        let v = e.lock().unwrap();
        (v.reserve, v.commit, v.release, v.positive_work)
    };
    retired(&r, &e, None);
    assert!(counts.0 > 1 && counts.1 > 1 && counts.2 > 1 && counts.3 > 256);
    let faults = (1..=counts.0)
        .map(Fault::Reserve)
        .chain((1..=counts.1).map(Fault::Commit))
        .chain((1..=counts.2).map(Fault::Release));
    for fault in faults {
        let (r, e) = resources(Some(fault), Limits::default());
        let mut p = Checked::new(&r, &e, Cache::Miss);
        assert_eq!(
            checked(VARIABLE, 19, &coords, &r, &mut p).err(),
            Some(FAULT),
            "{fault:?}"
        );
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(e.lock().unwrap().injected, 1);
        assert_eq!(p.after_failure, 0);
        assert!(p.aborts <= 1);
        let before = e.lock().unwrap().work.len();
        let font = FontRef::new(VARIABLE).unwrap();
        assert_eq!(ColorGlyphs::try_new(&font, &r).err(), Some(FAULT));
        assert_eq!(e.lock().unwrap().work.len(), before);
        retired(&r, &e, Some(fault));
    }
    println!("COLOR 257 var sweep real reserves/commits/releases/positive_work={counts:?}; all allocation/refund ordinals hit");
}

#[test]
fn every_positive_work_ordinal_in_real_257_var_paint_is_typed_and_sticky() {
    let coords = [F2Dot14::from_f32(0.5)];
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    checked(VARIABLE, 19, &coords, &r, &mut p).unwrap().unwrap();
    let count = e.lock().unwrap().positive_work;
    retired(&r, &e, None);
    assert!(count > 256);
    for ordinal in 1..=count {
        let (r, e) = resources(Some(Fault::Work(ordinal)), Limits::default());
        let mut p = Checked::new(&r, &e, Cache::Miss);
        assert_eq!(
            checked(VARIABLE, 19, &coords, &r, &mut p).err(),
            Some(FAULT),
            "work ordinal {ordinal}"
        );
        let v = e.lock().unwrap();
        assert_eq!(v.injected, 1);
        assert_eq!(v.positive_work, ordinal);
        drop(v);
        assert_eq!(p.after_failure, 0);
        assert!(p.aborts <= 1);
        retired(&r, &e, Some(Fault::Work(ordinal)));
    }
    println!("COLOR actual 257 variable stop paint work ordinals 1..={count} all hit; no later painter callback");
}

#[test]
fn callback_first_slot_blocks_every_following_transform_clip_layer_and_fill() {
    for id in [2, 4, 5, 6, 8] {
        let (_, plain) = plain(GRAPH, id, &[], Cache::Miss);
        for ordinal in 1..=plain.ops.len() {
            for returns_ok in [false, true] {
                let (r, e) = resources(None, Limits::default());
                let mut p = Checked::new(&r, &e, Cache::Miss);
                p.fault_at = Some(ordinal);
                p.fault_returns_ok = returns_ok;
                assert_eq!(checked(GRAPH, id, &[], &r, &mut p).err(), Some(OP_ERROR));
                assert_eq!(p.ops.len(), ordinal);
                assert_eq!(p.after_failure, 0);
                assert_eq!(p.aborts, 1);
                retired(&r, &e, None);
            }
        }
    }
}

#[test]
fn cache_callback_last_natural_drop_beats_hit_miss_and_font_error() {
    for cache in [Cache::Hit, Cache::Miss, Cache::FontError] {
        let (r, e) = resources(None, Limits::default());
        let mut p = Checked::new(&r, &e, cache);
        p.temporary_on_cache = true;
        let _ = checked(GRAPH, 2, &[], &r, &mut p).unwrap();
        assert!(!p.temporary_releases.is_empty());
        let ordinal = p.temporary_releases[0];
        retired(&r, &e, None);
        let (r, e) = resources(Some(Fault::Release(ordinal)), Limits::default());
        let mut p = Checked::new(&r, &e, cache);
        p.temporary_on_cache = true;
        assert_eq!(checked(GRAPH, 2, &[], &r, &mut p).err(), Some(FAULT));
        assert_eq!(p.ops, [Op::Cache(3)]);
        assert_eq!(p.aborts, 1);
        assert_eq!(p.after_failure, 0);
        assert_eq!(e.lock().unwrap().injected, 1);
        retired(&r, &e, Some(Fault::Release(ordinal)));
    }
}

#[test]
fn final_stop_heap_natural_drop_and_callback_drop_reject_success_before_next_glyph() {
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    checked(NORMAL, 19, &[], &r, &mut p).unwrap().unwrap();
    let final_release = e.lock().unwrap().release;
    assert!(final_release > 1);
    let plain_ops = p.ops;
    retired(&r, &e, None);
    let (r, e) = resources(Some(Fault::Release(final_release)), Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    assert_eq!(checked(NORMAL, 19, &[], &r, &mut p).err(), Some(FAULT));
    assert_eq!(p.ops, plain_ops);
    assert_eq!(p.aborts, 1);
    retired(&r, &e, Some(Fault::Release(final_release)));
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    p.temporary_on_event = true;
    checked(GRAPH, 4, &[], &r, &mut p).unwrap().unwrap();
    let ordinal = p.temporary_releases[0];
    retired(&r, &e, None);
    let (r, e) = resources(Some(Fault::Release(ordinal)), Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    p.temporary_on_event = true;
    assert_eq!(checked(GRAPH, 4, &[], &r, &mut p).err(), Some(FAULT));
    assert_eq!(p.ops.len(), 1);
    assert_eq!(p.aborts, 1);
    retired(&r, &e, Some(Fault::Release(ordinal)));
}

#[test]
fn earlier_callback_error_survives_stop_heap_refund_failure() {
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    checked(NORMAL, 19, &[], &r, &mut p).unwrap().unwrap();
    let last = e.lock().unwrap().release;
    retired(&r, &e, None);
    let (r, e) = resources(Some(Fault::Release(last)), Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    p.fault_at = Some(1);
    assert_eq!(checked(NORMAL, 19, &[], &r, &mut p).err(), Some(OP_ERROR));
    assert_eq!(r.failure(), Some(OP_ERROR));
    assert_eq!(e.lock().unwrap().injected, 1);
    assert_eq!(p.aborts, 1);
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(r.validate_retired(), Err(OP_ERROR));
}

#[test]
fn resource_failure_in_unsupported_optimization_probe_has_no_second_traversal() {
    let (r, e) = resources(None, Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    checked(GRAPH, 5, &[], &r, &mut p).unwrap().unwrap();
    let boundary = p.first_callback_work.unwrap();
    assert!(boundary > 1);
    retired(&r, &e, None);
    let (r, e) = resources(Some(Fault::Work(boundary - 1)), Limits::default());
    let mut p = Checked::new(&r, &e, Cache::Miss);
    assert_eq!(checked(GRAPH, 5, &[], &r, &mut p).err(), Some(FAULT));
    assert!(p.ops.is_empty());
    assert_eq!(p.aborts, 1);
    assert_eq!(e.lock().unwrap().injected, 1);
    retired(&r, &e, Some(Fault::Work(boundary - 1)));
}

#[test]
fn palette_metadata_and_empty_collection_use_checked_readers() {
    for data in [
        NORMAL,
        font_test_data::COLRV0V1,
        font_test_data::VAZIRMATN_VAR,
    ] {
        let font = FontRef::new(data).unwrap();
        let (r, e) = resources(None, Limits::default());
        let a = font.color_palettes();
        let b = CheckedPalettes::try_new(&font, &r).unwrap();
        assert_eq!(a.len(), b.len());
        assert_eq!(a.is_empty(), b.is_empty());
        for index in 0..=a.len() {
            let x = a.get(index);
            let y = b.try_get(index).unwrap();
            assert_eq!(x.is_some(), y.is_some());
            if let (Some(x), Some(y)) = (x, y) {
                assert_eq!(x.colors(), y.colors());
                assert_eq!(x.index(), y.index());
                assert_eq!(x.palette_type(), y.try_palette_type().unwrap());
                assert_eq!(x.label(), y.try_label().unwrap());
            }
        }
        for index in [0, 1, 14, u16::MAX] {
            assert_eq!(a.color_label(index), b.try_color_label(index).unwrap());
        }
        let g = ColorGlyphs::try_new(&font, &r).unwrap();
        assert_eq!(
            font.color_glyphs().get(GlyphId::new(0)).is_some(),
            g.try_get(GlyphId::new(0)).unwrap().is_some()
        );
        r.record(FAULT);
        assert_eq!(b.try_get(0).err(), Some(FAULT));
        assert_eq!(g.try_get(GlyphId::new(u32::MAX)).err(), Some(FAULT));
        retired(&r, &e, None);
    }
}

#[test]
fn checked_painter_foreign_request_cannot_rescue_either_first_slot() {
    let (a, ea) = resources(None, Limits::default());
    let (b, eb) = resources(None, Limits::default());
    let mut p = Checked::new(&b, &eb, Cache::Miss);
    assert!(checked(NORMAL, 2, &[], &a, &mut p).is_err());
    assert!(a.failure().is_some());
    assert!(b.failure().is_some());
    assert!(p.ops.is_empty());
    assert_eq!(p.aborts, 0);
    retired(&a, &ea, None);
    retired(&b, &eb, None);
    let (a, ea) = resources(None, Limits::default());
    let (b, eb) = resources(None, Limits::default());
    b.record(FAULT);
    let mut p = Checked::new(&b, &eb, Cache::Miss);
    assert_eq!(checked(NORMAL, 2, &[], &a, &mut p).err(), Some(FAULT));
    assert_eq!(a.failure(), Some(FAULT));
    assert!(p.ops.is_empty());
    retired(&a, &ea, None);
    retired(&b, &eb, None);
}

#[test]
fn real_variable_stop_cursor_rejects_257th_before_returning_it() {
    fn run(fault: Option<Fault>, limit: usize) -> (usize, usize, Option<Error>) {
        let (r, e) = resources(fault, Limits::default());
        let font = FontRef::new(VARIABLE).unwrap();
        let colr = font.colr().unwrap();
        let coords = [F2Dot14::from_f32(0.5)];
        let instance =
            instance::ColrInstance::try_new(colr.clone(), &coords, &mut RequestControl(&r))
                .unwrap();
        let (raw, _) = colr.v1_base_glyph(GlyphId::new(19)).unwrap().unwrap();
        let read_fonts::tables::colr::Paint::Glyph(raw) = raw else {
            panic!("fixture root")
        };
        let paint = raw.paint().unwrap();
        let resolved = instance::try_resolve_paint(&instance, &paint, &mut RequestControl(&r))
            .unwrap()
            .unwrap();
        let stops = match resolved {
            instance::ResolvedPaint::SweepGradient { color_stops, .. } => color_stops,
            _ => panic!("fixture gradient"),
        };
        let mut cursor = stops.cursor();
        let mut returned = 0;
        let mut error = None;
        for _ in 0..limit {
            match cursor.try_next(&instance, &mut RequestControl(&r)) {
                Ok(Some(stop)) => {
                    assert!(stop.offset.is_finite());
                    returned += 1
                }
                Ok(None) => break,
                Err(err) => {
                    error = Some(err);
                    break;
                }
            }
        }
        let work = e.lock().unwrap().positive_work;
        if error.is_some() {
            let before = e.lock().unwrap().work.len();
            assert_eq!(
                cursor.try_next(&instance, &mut RequestControl(&r)).err(),
                Some(FAULT)
            );
            assert_eq!(e.lock().unwrap().work.len(), before);
        }
        retired(&r, &e, fault);
        (returned, work, error)
    }
    let (count, prefix, error) = run(None, 256);
    assert_eq!(count, 256);
    assert_eq!(error, None);
    let (count, work, error) = run(Some(Fault::Work(prefix + 1)), 257);
    assert_eq!(count, 256);
    assert_eq!(work, prefix + 1);
    assert_eq!(error, Some(FAULT));
}

#[test]
fn artificial_nan_sort_is_explicitly_outside_fontdata_equivalence() {
    use crate::control::storage::SmallBuffer;
    use core::cmp::Ordering;
    let mut cx = Ordinary::new();
    let mut plain = cx.small::<ColorStop, 32>().unwrap();
    let (r, e) = resources(None, Limits::default());
    let mut checked = RequestControl(&r).small::<ColorStop, 32>().unwrap();
    for offset in [2.0, f32::NAN, 1.0] {
        let s = ColorStop {
            offset,
            palette_index: 0,
            alpha: 1.0,
        };
        plain.try_push(s).unwrap();
        checked.try_push(s).unwrap();
    }
    let mut cmp =
        |a: &ColorStop, b: &ColorStop| a.offset.partial_cmp(&b.offset).unwrap_or(Ordering::Equal);
    assert_eq!(cmp(&plain[0], &plain[1]), Ordering::Equal);
    cx.stable_sort::<ColorStop, 32>(&mut plain, &mut cmp)
        .unwrap();
    RequestControl(&r)
        .stable_sort::<ColorStop, 32>(&mut checked, &mut cmp)
        .unwrap();
    let a = plain.iter().map(|s| s.offset.to_bits()).collect::<Vec<_>>();
    let b = checked
        .iter()
        .map(|s| s.offset.to_bits())
        .collect::<Vec<_>>();
    assert_ne!(a, b, "no claim of arbitrary NaN sort equivalence");
    drop(checked);
    retired(&r, &e, None);
}

#[test]
fn plan_final_pad_extra_stop_all_reserve_commit_release_ordinals_are_sensitive() {
    let mut injected = 0;
    for id in [2, 5, 8, 11, 14, 17] {
        let (r, e) = resources(None, Limits::default());
        let mut p = Checked::new(&r, &e, Cache::Miss);
        checked(PAD, id, &[], &r, &mut p).unwrap().unwrap();
        let counts = {
            let v = e.lock().unwrap();
            (v.reserve, v.commit, v.release, v.positive_work)
        };
        retired(&r, &e, None);
        assert!(counts.0 > 0 && counts.1 > 0 && counts.2 > 0);
        let faults = (1..=counts.0)
            .map(Fault::Reserve)
            .chain((1..=counts.1).map(Fault::Commit))
            .chain((1..=counts.2).map(Fault::Release))
            .chain([Fault::Work(1), Fault::Work(counts.3)]);
        for fault in faults {
            let (r, e) = resources(Some(fault), Limits::default());
            let mut p = Checked::new(&r, &e, Cache::Miss);
            assert_eq!(
                checked(PAD, id, &[], &r, &mut p).err(),
                Some(FAULT),
                "pad gid {id} {fault:?}"
            );
            assert_eq!(e.lock().unwrap().injected, 1);
            assert_eq!(p.after_failure, 0);
            assert!(p.aborts <= 1);
            retired(&r, &e, Some(fault));
            injected += 1;
        }
        println!("COLOR final Pad gid={id} reserve/commit/release/work={counts:?}; actual extra stop uses paid capacity");
    }
    println!("COLOR final Pad actual sensitive fault cases={injected}");
}

#[test]
fn plan_final_v0_and_nested_v1_width_stop_before_the_257th_layer_callback() {
    for (data, codepoint) in [(WIDTH_V0, '\u{e600}'), (WIDTH_V1, '\u{e601}')] {
        let font = FontRef::new(data).unwrap();
        assert_eq!(font.charmap().map(codepoint), Some(GlyphId::new(2)));
        let (a, plain) = plain(data, 2, &[], Cache::Miss);
        a.unwrap();
        assert_eq!(plain.ops.len(), 257);
        let (r, e) = resources(None, Limits::default());
        let mut p = Checked::new(&r, &e, Cache::Miss);
        checked(data, 2, &[], &r, &mut p).unwrap().unwrap();
        assert_eq!(p.ops, plain.ops);
        assert_eq!(p.callback_work.len(), 257);
        let ordinal = p.callback_work[256];
        retired(&r, &e, None);
        let (r, e) = resources(Some(Fault::Work(ordinal)), Limits::default());
        let mut p = Checked::new(&r, &e, Cache::Miss);
        assert_eq!(checked(data, 2, &[], &r, &mut p).err(), Some(FAULT));
        assert_eq!(p.ops.len(), 256);
        assert_eq!(p.after_failure, 0);
        assert_eq!(p.aborts, 1);
        assert_eq!(e.lock().unwrap().injected, 1);
        retired(&r, &e, Some(Fault::Work(ordinal)));
        println!("COLOR final width source U+{:X} 257th callback work ordinal={ordinal}; returned callbacks=256",codepoint as u32);
    }
}

// Minimal public-path regression supplied by the independent implementation review.
fn max_layer_index_font() -> Vec<u8> {
    let mut colr = vec![0u8; 50];
    colr[..2].copy_from_slice(&1u16.to_be_bytes());
    colr[14..18].copy_from_slice(&34u32.to_be_bytes());
    colr[34..38].copy_from_slice(&1u32.to_be_bytes());
    colr[38..40].copy_from_slice(&2u16.to_be_bytes());
    colr[40..44].copy_from_slice(&10u32.to_be_bytes());
    colr[44..50].copy_from_slice(&[1, 1, 255, 255, 255, 255]);
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    sfnt(vec![(*b"COLR", colr), (*b"head", head)])
}

#[test]
fn max_layer_index_ordinary_returns_font_error() {
    let (result, painter) = plain(&max_layer_index_font(), 2, &[], Cache::Miss);
    #[cfg(target_pointer_width = "32")]
    assert!(matches!(
        result,
        Err(PaintError::ParseError(ReadError::OutOfBounds))
    ));
    #[cfg(not(target_pointer_width = "32"))]
    assert!(result.is_err());
    assert!(painter.ops.is_empty());
}

#[test]
fn max_layer_index_checked_returns_font_error() {
    let (r, e) = resources(None, Limits::default());
    let mut painter = Checked::new(&r, &e, Cache::Miss);
    let result = checked(&max_layer_index_font(), 2, &[], &r, &mut painter);
    #[cfg(target_pointer_width = "32")]
    assert!(matches!(
        result,
        Ok(Err(PaintError::ParseError(ReadError::OutOfBounds)))
    ));
    #[cfg(not(target_pointer_width = "32"))]
    assert!(result.unwrap().is_err());
    assert!(painter.ops.is_empty());
    assert_eq!(painter.aborts, 1);
    retired(&r, &e, None);
}
