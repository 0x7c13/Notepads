// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual outline drawing and the shared span/variation geometry helpers.

use super::*;
use crate::geometry_test_support::{native, request, Call};
use markdown_svg_budget::{FailureKind, ResourceKind};

fn font(with_opsz: bool) -> (Arc<Database>, ID) {
    let original = include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf");
    let data = if with_opsz {
        // Original test-only SFNT directory mutation: preserve every existing
        // Tuffy table byte, add one uninstanced opsz axis. No expected output or
        // fixed upstream corpus is modified, and this is not a variable-font
        // outline compatibility claim.
        let count = u16::from_be_bytes(original[4..6].try_into().unwrap()) as usize;
        let table_end = 12 + count * 16;
        let mut data = original.to_vec();
        data.splice(table_end..table_end, [0u8; 16]);
        data[4..6].copy_from_slice(&((count + 1) as u16).to_be_bytes());
        for at in (12..table_end).step_by(16) {
            let offset = u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap());
            data[at + 8..at + 12].copy_from_slice(&(offset + 16).to_be_bytes());
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let offset = data.len() as u32;
        let mut fvar = Vec::new();
        for value in [1u16, 0, 16, 2, 1, 20, 0, 8] {
            fvar.extend_from_slice(&value.to_be_bytes());
        }
        fvar.extend_from_slice(b"opsz");
        for value in [8i32 << 16, 12 << 16, 72 << 16] {
            fvar.extend_from_slice(&value.to_be_bytes());
        }
        fvar.extend_from_slice(&[0, 0, 1, 0]);
        assert_eq!(fvar.len(), 36);
        data[table_end..table_end + 4].copy_from_slice(b"fvar");
        data[table_end + 8..table_end + 12].copy_from_slice(&offset.to_be_bytes());
        data[table_end + 12..table_end + 16].copy_from_slice(&(fvar.len() as u32).to_be_bytes());
        data.extend_from_slice(&fvar);
        let mut records = data[12..table_end + 16]
            .chunks_exact(16)
            .map(|v| <[u8; 16]>::try_from(v).unwrap())
            .collect::<Vec<_>>();
        records.sort_by_key(|v| <[u8; 4]>::try_from(&v[..4]).unwrap());
        for (at, record) in records.iter().enumerate() {
            data[12 + at * 16..12 + (at + 1) * 16].copy_from_slice(record);
        }
        data
    } else {
        original.to_vec()
    };
    let mut db = Database::new();
    db.load_font_data(data);
    let id = db.faces().next().expect("Tuffy fixture face").id;
    (Arc::new(db), id)
}

fn first_text(group: &Group) -> Option<&Text> {
    for child in &group.children {
        match child {
            Node::Text(text) => return Some(text),
            Node::Group(group) => {
                if let Some(text) = first_text(group) {
                    return Some(text);
                }
            }
            _ => (),
        }
    }
    None
}

fn stock_fixture(with_opsz: bool) -> (Arc<Database>, ID, layout::Span) {
    let (db, id) = font(with_opsz);
    let options = Options {
        fontdb: db.clone(),
        font_family: "Tuffy".into(),
        ..Options::default()
    };
    let tree = Tree::from_data_in_request(b"<svg xmlns='http://www.w3.org/2000/svg' width='120' height='80'><text x='2' y='30' font-family='Tuffy' font-size='16'>AB</text></svg>", &options).unwrap();
    let span = first_text(tree.root()).expect("actual stock text").layouted[0]
        .try_copy_in(&ProcessingContext::default())
        .unwrap();
    assert_eq!(span.positioned_glyphs[0].font, id);
    (db, id, span)
}

#[test]
fn actual_skrifa_outline_ordinary_and_paid_have_identical_verbs_points_and_bounds() {
    let (db, _) = font(false);
    let id = db.faces().next().unwrap().id;
    let glyph = db
        .with_face_data(id, |data, index| {
            let font = skrifa::FontRef::from_index(data, index).unwrap();
            GlyphId(
                skrifa::charmap::Charmap::new(&font)
                    .map('A')
                    .unwrap()
                    .to_u32(),
            )
        })
        .unwrap();
    let ordinary = match db
        .owned_outline(id, glyph, &[], None, &ProcessingContext::default())
        .unwrap()
    {
        OwnedOutline::NonEmpty(path) => path,
        _ => panic!("Tuffy A is a real nonempty draw"),
    };
    let (resources, state) = request(Default::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let paid = match db.owned_outline(id, glyph, &[], None, &processing).unwrap() {
        OwnedOutline::NonEmpty(path) => path,
        _ => panic!("controlled draw must preserve representation"),
    };
    assert!(paid.belongs_to(&resources));
    assert_eq!(paid.retained_unpaid_capacity_bytes(), Some(0));
    assert_eq!(ordinary.verbs(), paid.verbs());
    assert_eq!(ordinary.points(), paid.points());
    assert_eq!(ordinary.bounds(), paid.bounds());
    assert!(state.counts().reserve >= 3);
    drop(paid);
    assert_eq!(state.live_len(), 0);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[test]
fn actual_skrifa_allocation_failures_remain_typed_in_result_and_old_option_boundary() {
    let (db, id) = font(false);
    let glyph = db
        .with_face_data(id, |data, index| {
            let font = skrifa::FontRef::from_index(data, index).unwrap();
            GlyphId(
                skrifa::charmap::Charmap::new(&font)
                    .map('A')
                    .unwrap()
                    .to_u32(),
            )
        })
        .unwrap();
    let (resources, state) = request(Default::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    drop(db.owned_outline(id, glyph, &[], None, &processing).unwrap());
    let baseline = state.counts();
    drop(processing);
    resources.validate_retired().unwrap();
    let mut cases = 0;
    for call in [Call::Reserve, Call::Commit] {
        let count = if call == Call::Reserve {
            baseline.reserve
        } else {
            baseline.commit
        };
        for at in 1..=count {
            for old_option in [true, false] {
                // Each entry receives a fresh request, so try_ cannot prefill
                // the first-fault slot and make an old boundary falsely pass.
                let (resources, state) = request(Default::default());
                let processing = ProcessingContext::with_resources(resources.clone());
                state.arm(call, at, native(-823));
                if old_option {
                    assert!(db.outline(id, glyph, &[], None, &processing).is_none());
                } else {
                    assert!(
                        matches!(db.owned_outline(id, glyph, &[], None, &processing), Err(error) if error == native(-823))
                    );
                }
                assert_eq!(resources.failure(), Some(native(-823)));
                assert_eq!(processing.resource_failure(), Some(native(-823)));
                let events = state.lock();
                let index = events
                    .events
                    .iter()
                    .position(|event| event.call == call && event.ordinal == at)
                    .unwrap();
                assert!(events.events[index + 1..]
                    .iter()
                    .all(|event| event.call == Call::Release));
                drop(events);
                drop(processing);
                assert_eq!(state.live_len(), 0);
                resources.validate_retired().unwrap();
                cases += 1;
            }
        }
    }
    println!(
        "ACTUAL_TUFFY_OUTLINE_FAULTS reserves={} commits={} fresh_entry_cases={cases}",
        baseline.reserve, baseline.commit
    );
}

#[derive(Debug)]
struct AxesOwner {
    axes: Arc<[FontVariation]>,
    observed_geometry: std::sync::atomic::AtomicUsize,
}
impl FontReplay for AxesOwner {
    fn prepare(&self, _: &Text, _: &mut Arc<Database>) -> Result<ReplayText, ()> {
        panic!("axes lookup cannot prepare")
    }
    fn fontdb(&self) -> Result<Arc<Database>, ()> {
        panic!("axes lookup cannot read font data")
    }
    fn variations(&self, _: ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(self.axes.clone())
    }
    fn outline(&self, _: ID, _: u32) -> Result<ReplayOutline, ()> {
        panic!("axes lookup cannot select outlines")
    }
    fn observe_geometry(&self, _: ReplayGeometryObservation<'_>) -> Result<(), ()> {
        self.observed_geometry
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn replay_axes_are_shared_without_adding_automatic_opsz_or_copying_long_input() {
    let (db, _, span) = stock_fixture(true);
    let axes: Arc<[FontVariation]> = (0..600)
        .map(|i| FontVariation::new((i as u32).to_be_bytes(), i as f32))
        .collect::<Vec<_>>()
        .into();
    let owner = Arc::new(AxesOwner {
        axes: axes.clone(),
        observed_geometry: Default::default(),
    });
    let (resources, state) = request(Default::default());
    let mut cache = Cache::with_processing(
        Arc::new(ProcessingContext::with_resources(resources.clone())),
        db,
    );
    cache.font_replay = Some(owner);
    let value = try_effective_variations(&mut cache, &span, &span.positioned_glyphs[0]).unwrap();
    assert!(matches!(value, EffectiveAxes::SourceShared(_)));
    assert_eq!(value.len(), 600);
    assert_eq!(value.as_ptr(), axes.as_ptr());
    assert!(value.iter().all(|value| value.tag != *b"opsz"));
    assert_eq!(state.counts().reserve, 0);
    retire_effective_axes(value, &cache.processing).unwrap();
    drop(cache);
    resources.validate_retired().unwrap();
}

#[test]
fn foreign_paid_synthetic_stops_before_observer_or_span_mutation() {
    let (db, _, span) = stock_fixture(false);
    let (source_resources, source_state) = request(Default::default());
    let data = crate::geometry::SharedOwner::try_new(
        crate::geometry_test_support::outline()
            .try_copy_in(&source_resources, ResourceKind::Geometry)
            .unwrap(),
        Some(&source_resources),
        ResourceKind::Geometry,
    )
    .unwrap();
    let (resources, state) = request(Default::default());
    let mut cache = Cache::with_processing(
        Arc::new(ProcessingContext::with_resources(resources.clone())),
        db,
    );
    let owner = Arc::new(AxesOwner {
        axes: Arc::from([]),
        observed_geometry: Default::default(),
    });
    cache.font_replay = Some(owner.clone());
    let mut builder = span_builder(&cache.processing);
    let mut children = TreeVec::new(&cache.processing);
    let mut capture = Capture::new(&cache).unwrap();
    assert!(flatten_synthetic(
        Some(
            data.try_alias_in(&ProcessingContext::with_resources(source_resources.clone()))
                .unwrap()
        ),
        FillRule::NonZero,
        &span.positioned_glyphs[0],
        &span,
        "foreign-input",
        &mut cache,
        &mut builder,
        &mut children,
        ShapeRendering::default(),
        Transform::identity(),
        &mut capture
    )
    .is_none());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
    assert_eq!(
        owner
            .observed_geometry
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(children.is_empty());
    assert_eq!(state.counts().reserve, 0);
    assert_eq!(
        source_resources.failure().unwrap().kind,
        FailureKind::Invariant
    );
    drop((builder, capture, children, cache));
    resources.validate_retired().unwrap();
    assert_eq!(state.live_len(), 0);
    drop(data);
    assert_eq!(source_state.live_len(), 0);
    source_resources.validate_retired().unwrap();
}

#[test]
fn outline_pen_keeps_default_semantics_and_does_not_execute_commands_after_failure() {
    let (resources, state) = request(Default::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let mut paid = PathBuilder::with_processing(&processing);
    let mut ordinary = PathBuilder::default();
    for pen in [&mut ordinary, &mut paid] {
        pen.line_to(5.0, 6.0);
        pen.close();
        pen.move_to(1.0, 2.0);
        pen.curve_to(2.0, 4.0, 3.0, 7.0, 8.0, 9.0);
        pen.close();
    }
    assert_eq!(ordinary.commands, paid.commands);
    let ordinary = ordinary.builder.try_finish().unwrap().unwrap();
    let paid = paid.builder.try_finish().unwrap().unwrap();
    assert_eq!(ordinary.verbs(), paid.verbs());
    assert_eq!(ordinary.points(), paid.points());
    drop(paid);
    assert_eq!(state.live_len(), 0);
    drop(processing);
    resources.validate_retired().unwrap();

    let (resources, state) = request(Default::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let mut paid = PathBuilder::with_processing(&processing);
    state.arm(Call::Reserve, 1, native(-820));
    paid.move_to(1.0, 2.0);
    let counts = state.counts();
    assert_eq!(paid.failure, Some(native(-820)));
    for _ in 0..500 {
        paid.line_to(4.0, 5.0);
        paid.quad_to(1.0, 2.0, 3.0, 4.0);
        paid.curve_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
        paid.close();
    }
    assert_eq!(paid.commands, 1);
    assert_eq!(state.counts().reserve, counts.reserve);
    assert_eq!(state.counts().checkpoint, counts.checkpoint);
    assert_eq!(resources.failure(), Some(native(-820)));
    drop(paid);
    assert_eq!(state.live_len(), 0);
    drop(processing);
    resources.validate_retired().unwrap();
}

#[test]
fn effective_axes_only_copy_for_added_opsz_and_retire_before_next_consumer() {
    let (db, id, mut span) = stock_fixture(true);
    assert!(db.has_opsz_axis(id, &ProcessingContext::default()));
    span.font_optical_sizing = FontOpticalSizing::Auto;
    span.variations = TreeVec::plain(
        (0..600)
            .map(|i| FontVariation::new((i as u32).to_be_bytes(), i as f32))
            .collect(),
    );
    let glyph = &span.positioned_glyphs[0];
    let (resources, state) = request(Default::default());
    let mut cache = Cache::with_processing(
        Arc::new(ProcessingContext::with_resources(resources.clone())),
        db.clone(),
    );
    let value = try_effective_variations(&mut cache, &span, glyph).unwrap();
    assert!(matches!(value, EffectiveAxes::Paid(_)));
    assert_eq!(value.len(), 601);
    assert_eq!(&value[..600], &span.variations[..]);
    assert_eq!(value[600], FontVariation::new(*b"opsz", glyph.font_size()));
    assert!(
        state
            .lock()
            .events
            .iter()
            .filter(|e| e.call == Call::Checkpoint && e.work == 256)
            .count()
            >= 2
    );
    retire_effective_axes(value, &cache.processing).unwrap();
    drop(cache);
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();

    let (resources, state) = request(Default::default());
    let mut cache = Cache::with_processing(
        Arc::new(ProcessingContext::with_resources(resources.clone())),
        db,
    );
    let value = try_effective_variations(&mut cache, &span, glyph).unwrap();
    state.arm(Call::Release, 1, native(-821));
    assert!(retire_effective_axes(value, &cache.processing).is_none());
    assert_eq!(resources.failure(), Some(native(-821)));
    drop(cache);
    assert_eq!(state.live_len(), 1);
    assert_eq!(resources.statistics().release_failures, 1);
    assert!(resources.validate_retired().is_err());

    for automatic in [true, false] {
        let (db, _, mut span) = stock_fixture(automatic);
        if automatic {
            span.variations
                .try_push(
                    FontVariation::new(*b"opsz", 14.0),
                    &ProcessingContext::default(),
                )
                .unwrap();
        }
        let (resources, state) = request(Default::default());
        let mut cache = Cache::with_processing(
            Arc::new(ProcessingContext::with_resources(resources.clone())),
            db,
        );
        let value =
            try_effective_variations(&mut cache, &span, &span.positioned_glyphs[0]).unwrap();
        assert!(matches!(value, EffectiveAxes::Borrowed(_)));
        assert_eq!(&*value, &span.variations[..]);
        retire_effective_axes(value, &cache.processing).unwrap();
        drop(cache);
        assert_eq!(state.live_len(), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn span_append_cleanup_failure_stops_before_next_glyph_or_path_publication() {
    let (resources, state) = request(Default::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let source = crate::geometry_test_support::outline()
        .try_copy_in(&resources, ResourceKind::Scratch)
        .unwrap();
    let mut builder = span_builder(&processing);
    state.arm(Call::Release, 1, native(-822));
    assert!(append_outline(&mut builder, Some(source), &processing).is_none());
    assert_eq!(resources.failure(), Some(native(-822)));
    assert!(geometry_ready(&processing).is_none());
    let before = state.counts();
    assert!(builder.try_finish().is_err());
    assert_eq!(state.counts().reserve, before.reserve);
    assert_eq!(state.counts().checkpoint, before.checkpoint);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(state.live_len(), 1);
    drop(processing);
    assert!(resources.validate_retired().is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Native);
}
