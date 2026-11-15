// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Outline-cache helper tests only; not actual shaping/Native-consumer qualification.

use super::*;
use crate::geometry_test_support::{
    AllocationObservation, Call, State, cancelled, native, outline, request,
};
use markdown_svg_budget::Limits;
use std::sync::Arc;

// Independent oracle for the fixed hashbrown 0.17.1 control/group selection.
// SSE2 scans 16 bytes; NEON scans 8. Generic uses u64 on the targets below
// (including wasm32), and u32 otherwise. The dependency enables allocator-api2
// only, so hashbrown's separate nightly-only LSX implementation is not selected.
const CTRL_GROUP_BYTES: u64 = if cfg!(all(
    target_feature = "sse2",
    any(target_arch = "x86", target_arch = "x86_64"),
    not(miri),
)) {
    16
} else if cfg!(any(
    target_pointer_width = "64",
    target_arch = "aarch64",
    target_arch = "x86_64",
    target_arch = "wasm32",
)) {
    8
} else {
    4
};

fn axes(count: usize) -> Vec<FontVariation> {
    (0..count)
        .map(|i| FontVariation::new((i as u32).to_be_bytes(), i as f32 * 0.25))
        .collect()
}

fn put(cache: &mut OutlineCache, glyph: u32, axes: &[FontVariation], value: Outline) {
    let (query, _) = cache
        .lookup(fontdb::ID::dummy(), GlyphId(glyph), axes)
        .unwrap();
    cache.insert(query, value).unwrap();
}

fn paid_table(cache: &OutlineCache) -> &PaidShared<Table> {
    match cache {
        OutlineCache::Paid {
            table: Some(table), ..
        } => table,
        _ => panic!("expected initialized paid cache"),
    }
}

fn seeded_cache(r: &ResourcesRef, builder: &RandomState) -> OutlineCache {
    let mut cache = OutlineCache::new(Some(r));
    match &mut cache {
        OutlineCache::Paid { hasher, .. } => *hasher = builder.clone(),
        _ => unreachable!(),
    }
    cache
}

#[test]
fn tuple_hash_matches_actual_same_seed_vec_hash_for_bitwise_axes_and_order() {
    let (r, state) = request(Limits::default());
    let mut variants = vec![
        vec![],
        vec![FontVariation::new(*b"wght", 0.0)],
        vec![FontVariation::new(*b"wght", -0.0)],
        vec![FontVariation::new(*b"wght", f32::from_bits(0x7fc0_1234))],
        vec![FontVariation::new(*b"wght", f32::from_bits(0x7fc0_1235))],
        vec![
            FontVariation::new(*b"wght", 300.0),
            FontVariation::new(*b"wdth", 80.0),
        ],
        vec![
            FontVariation::new(*b"wdth", 80.0),
            FontVariation::new(*b"wght", 300.0),
        ],
        axes(255),
        axes(256),
        axes(257),
        axes(600),
    ];
    variants.push(vec![FontVariation::new(*b"wght", f32::NEG_INFINITY)]);
    for seed in 0..4 {
        let builder = RandomState::new();
        for font in [fontdb::ID::dummy(), fontdb::ID::default()] {
            for glyph in [GlyphId(0), GlyphId(17), GlyphId(u32::MAX)] {
                for values in &variants {
                    let mut original = builder.build_hasher();
                    // This executes the original tuple + Vec Hash, including the
                    // actual toolchain's slice length prefix and per-axis hash.
                    (font, glyph, values.clone()).hash(&mut original);
                    assert_eq!(
                        key_hash(&r, &builder, font, glyph, values).unwrap(),
                        original.finish(),
                        "same-seed tuple mismatch seed={seed} axes={}",
                        values.len()
                    );
                }
            }
        }
    }
    state.assert_live(&r, 0);
    println!("CACHE_TUPLE_HASH seeds=4 fonts=2 glyphs=3 axesVariants=12 comparisons=288");
}

#[test]
fn actual_lookup_distinguishes_miss_cached_none_and_bitwise_long_owned_keys() {
    for paid in [false, true] {
        let (r, state) = request(Limits::default());
        let mut cache = OutlineCache::new(paid.then_some(&r));
        let variants = vec![
            vec![],
            vec![FontVariation::new(*b"wght", 0.0)],
            vec![FontVariation::new(*b"wght", -0.0)],
            vec![FontVariation::new(*b"wght", f32::from_bits(0x7fc0_1234))],
            vec![FontVariation::new(*b"wght", f32::from_bits(0x7fc0_1235))],
            vec![
                FontVariation::new(*b"wght", 300.0),
                FontVariation::new(*b"wdth", 80.0),
            ],
            vec![
                FontVariation::new(*b"wdth", 80.0),
                FontVariation::new(*b"wght", 300.0),
            ],
            axes(600),
        ];
        for values in &variants {
            let (query, found) = cache
                .lookup(fontdb::ID::dummy(), GlyphId(7), values)
                .unwrap();
            assert!(found.is_none(), "distinct bitwise keys must miss");
            cache.insert(query, None).unwrap();
            let (_, found) = cache
                .lookup(fontdb::ID::dummy(), GlyphId(7), values)
                .unwrap();
            assert!(matches!(found, Some(None)), "absent outline is cached");
        }
        let mut caller_axes = axes(600);
        let original = caller_axes.clone();
        put(&mut cache, 11, &caller_axes, Some(outline()));
        caller_axes[0].value = 999.0;
        assert!(
            cache
                .lookup(fontdb::ID::dummy(), GlyphId(11), &caller_axes)
                .unwrap()
                .1
                .is_none()
        );
        let found = cache
            .lookup(fontdb::ID::dummy(), GlyphId(11), &original)
            .unwrap()
            .1
            .unwrap()
            .unwrap();
        assert_eq!(found.points(), outline().points());
        assert!(
            cache
                .lookup(fontdb::ID::default(), GlyphId(11), &original)
                .unwrap()
                .1
                .is_none()
        );
        drop(found);
        if paid {
            for (key, value) in paid_table(&cache).iter() {
                assert!(key.axes.belongs_to(&r));
                assert_eq!(key.axes.retained_unpaid_capacity(), 0);
                if let Some(path) = value {
                    assert!(path.belongs_to(&r));
                    assert_eq!(path.retained_unpaid_capacity_bytes(), Some(0));
                }
            }
            assert!(paid_table(&cache).allocator().belongs_to(&r));
            state.assert_live(&r, state.live_len());
        }
        drop(cache);
        state.assert_live(&r, 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn paid_hits_and_alias_clone_do_not_allocate_or_copy_real_system_keys() {
    let values = axes(600);
    let (r, state) = request(Limits::default());
    let mut cache = OutlineCache::new(Some(&r));
    put(&mut cache, 1, &values, Some(outline()));
    put(&mut cache, 2, &values, None);
    let before = r.statistics();
    let reserves = state.counts().reserve;
    // Validate that this observer sees actual allocator traffic on this thread.
    let observation = AllocationObservation::begin();
    let sensitivity = std::hint::black_box(Vec::<u8>::with_capacity(777));
    let detected = observation.finish();
    assert!(detected.alloc + detected.zeroed > 0);
    drop(sensitivity);
    let observation = AllocationObservation::begin();
    let found = cache
        .lookup(fontdb::ID::dummy(), GlyphId(1), &values)
        .unwrap();
    let absent = cache
        .lookup(fontdb::ID::dummy(), GlyphId(2), &values)
        .unwrap();
    let alias = cache.clone();
    let measured = observation.finish();
    assert_eq!(measured.alloc, 0);
    assert_eq!(measured.zeroed, 0);
    assert_eq!(measured.realloc, 0);
    assert!(found.1.as_ref().unwrap().as_ref().unwrap().belongs_to(&r));
    assert!(matches!(absent.1, Some(None)));
    match &found.0 {
        Query::Paid { axes: borrowed, .. } => {
            assert!(std::ptr::eq(borrowed.as_ptr(), values.as_ptr()))
        }
        _ => panic!("paid lookup must return a borrowed query"),
    }
    assert_eq!(state.counts().reserve, reserves);
    assert_eq!(r.statistics().scratch_bytes, before.scratch_bytes);
    assert_eq!(r.statistics().live_leases, before.live_leases);
    drop(found);
    drop(absent);
    drop(cache);
    assert_eq!(r.statistics().scratch_bytes, before.scratch_bytes);
    assert_eq!(r.statistics().live_leases, before.live_leases);
    drop(alias);
    state.assert_live(&r, 0);
    println!(
        "CACHE_HIT actualAlloc=0 actualZeroed=0 actualRealloc=0 sensitivityAlloc={}",
        detected.alloc
    );
}

#[test]
fn shared_cache_first_write_copies_only_table_and_preserves_source_owners() {
    let (r, state) = request(Limits::default());
    let mut source = OutlineCache::new(Some(&r));
    put(&mut source, 1, &axes(600), Some(outline()));
    put(&mut source, 2, &axes(3), None);
    let mut copy = source.clone();
    assert!(std::ptr::eq(&**paid_table(&source), &**paid_table(&copy)));
    let before = r.statistics();
    let source_path_pointer = paid_table(&source)
        .iter()
        .find(|(key, _)| key.glyph == GlyphId(1))
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .points()
        .as_ptr();
    put(&mut copy, 3, &axes(5), Some(outline()));
    assert!(!std::ptr::eq(&**paid_table(&source), &**paid_table(&copy)));
    assert_eq!(paid_table(&source).len(), 2);
    assert_eq!(paid_table(&copy).len(), 3);
    assert!(
        source
            .lookup(fontdb::ID::dummy(), GlyphId(3), &axes(5))
            .unwrap()
            .1
            .is_none()
    );
    let copied_entry = paid_table(&copy)
        .iter()
        .find(|(key, _)| key.glyph == GlyphId(1))
        .unwrap();
    assert_eq!(
        copied_entry.1.as_ref().unwrap().points().as_ptr(),
        source_path_pointer
    );
    let source_entry = paid_table(&source)
        .iter()
        .find(|(key, _)| key.glyph == GlyphId(1))
        .unwrap();
    assert_eq!(source_entry.0.axes.as_ptr(), copied_entry.0.axes.as_ptr());
    assert!(r.statistics().scratch_bytes > before.scratch_bytes);
    {
        let s = state.lock();
        assert!(s.peak_leases >= before.live_leases as usize + 4);
        assert!(s.events.iter().any(|e| e.call == Call::Commit
            && e.live_leases >= before.live_leases as usize + 4
            && e.bytes > 0
            && e.live_bytes > before.scratch_bytes as usize));
        assert!(s.peak_bytes >= r.statistics().scratch_bytes as usize);
    }
    drop(source);
    assert!(
        copy.lookup(fontdb::ID::dummy(), GlyphId(1), &axes(600))
            .unwrap()
            .1
            .unwrap()
            .unwrap()
            .belongs_to(&r)
    );
    drop(copy);
    state.assert_live(&r, 0);
}

#[test]
fn imported_source_path_kind_overlap_is_paid_and_foreign_request_is_rejected() {
    let (r, state) = request(Limits::default());
    let source = outline().try_copy_in(&r, ResourceKind::Geometry).unwrap();
    let source_bytes = state.kind_bytes(ResourceKind::Geometry);
    assert!(source_bytes > 0);
    assert_eq!(r.statistics().scratch_bytes, 0);
    let mut cache = OutlineCache::new(Some(&r));
    put(&mut cache, 9, &axes(10), Some(source.clone()));
    let cached = paid_table(&cache).iter().next().unwrap();
    assert!(cached.0.axes.belongs_to(&r));
    let cached = cached.1.as_ref().unwrap();
    assert!(cached.belongs_to(&r));
    assert_ne!(
        cached.points().as_ptr(),
        source.points().as_ptr(),
        "kind change makes independently paid storage"
    );
    assert_eq!(state.kind_bytes(ResourceKind::Geometry), source_bytes);
    assert!(state.kind_bytes(ResourceKind::Scratch) > 0);
    state.assert_account(&r);
    drop(cache);
    assert_eq!(state.kind_bytes(ResourceKind::Scratch), 0);
    assert_eq!(state.kind_bytes(ResourceKind::Geometry), source_bytes);
    assert_eq!(source.points(), outline().points());
    drop(source);
    state.assert_live(&r, 0);

    let (other, other_state) = request(Limits::default());
    let foreign = outline()
        .try_copy_in(&other, ResourceKind::Scratch)
        .unwrap();
    let original_bytes = other.statistics().scratch_bytes;
    let (r, state) = request(Limits::default());
    let mut cache = OutlineCache::new(Some(&r));
    let (query, found) = cache.lookup(fontdb::ID::dummy(), GlyphId(9), &[]).unwrap();
    assert!(found.is_none());
    let expected = Error::new(FailureKind::Invariant, LimitKind::None);
    assert_eq!(
        cache.insert(query, Some(foreign.clone())).err(),
        Some(expected)
    );
    assert_eq!(r.failure(), Some(expected));
    assert_eq!(other.failure(), None);
    assert_eq!(other.statistics().scratch_bytes, original_bytes);
    drop(cache);
    state.assert_live(&r, 0);
    assert_eq!(foreign.points(), outline().points());
    drop(foreign);
    other_state.assert_live(&other, 0);
    println!(
        "CACHE_SOURCE_KIND GeometryToScratchPaidCopy=true sourceAndCacheOverlap=true foreignRejected=true"
    );
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Cold,
    Cow,
    Replace,
}

fn prepare(
    operation: Operation,
    builder: &RandomState,
) -> (
    ResourcesRef,
    Arc<State>,
    OutlineCache,
    Option<OutlineCache>,
    tiny_skia_path::Path,
    Vec<FontVariation>,
) {
    let (r, state) = request(Limits::default());
    // Real source geometry stays charged while the cache builds Scratch owners.
    let source = outline().try_copy_in(&r, ResourceKind::Geometry).unwrap();
    let values = axes(600);
    let mut cache = seeded_cache(&r, builder);
    let alias = match operation {
        Operation::Cold => None,
        Operation::Replace => {
            put(&mut cache, 77, &values, Some(source.clone()));
            None
        }
        Operation::Cow => {
            for glyph in 1..=16 {
                put(
                    &mut cache,
                    glyph,
                    &axes(3),
                    (glyph == 1).then(|| source.clone()),
                );
            }
            Some(cache.clone())
        }
    };
    (r, state, cache, alias, source, values)
}

fn retire(r: &ResourcesRef, state: &State, expected_failed_refunds: usize) {
    state.assert_live(r, expected_failed_refunds);
    assert_eq!(
        r.statistics().release_failures as usize,
        expected_failed_refunds
    );
    if expected_failed_refunds == 0 && r.failure().is_none() {
        r.validate_retired().unwrap();
    }
}

#[test]
fn actual_cold_cow_and_replace_all_fault_ordinals_retire_and_keep_source_intact() {
    for operation in [Operation::Cold, Operation::Cow, Operation::Replace] {
        // Each exact ordinal replays the same seed and input, rather than
        // accidentally measuring a new collision/probe layout on every request.
        let builder = RandomState::new();
        let (r, state, mut cache, alias, source, values) = prepare(operation, &builder);
        let (query, _) = cache
            .lookup(fontdb::ID::dummy(), GlyphId(77), &values)
            .unwrap();
        let start = state.counts();
        cache.insert(query, Some(source.clone())).unwrap();
        let end = state.counts();
        let counts = [
            (Call::Reserve, end.reserve - start.reserve, native(-931)),
            (Call::Commit, end.commit - start.commit, native(-932)),
            (
                Call::Checkpoint,
                end.checkpoint - start.checkpoint,
                cancelled(),
            ),
            (Call::Release, end.release - start.release, native(-933)),
        ];
        drop(cache);
        drop(alias);
        drop(source);
        retire(&r, &state, 0);
        for (call, count, error) in counts {
            for ordinal in 1..=count {
                let (r, state, mut cache, alias, source, values) = prepare(operation, &builder);
                let (query, _) = cache
                    .lookup(fontdb::ID::dummy(), GlyphId(77), &values)
                    .unwrap();
                let source_points = source.points().to_vec();
                state.arm(call, ordinal, error);
                assert_eq!(
                    cache.insert(query, Some(source.clone())).err(),
                    Some(error),
                    "{operation:?} {call:?} ordinal={ordinal}"
                );
                assert_eq!(r.failure(), Some(error));
                assert_eq!(source.points(), source_points);
                if let Some(alias) = alias.as_ref() {
                    assert_eq!(paid_table(alias).len(), 16);
                }
                assert_eq!(
                    cache
                        .lookup(fontdb::ID::dummy(), GlyphId(77), &values)
                        .err(),
                    Some(error)
                );
                drop(cache);
                drop(alias);
                drop(source);
                retire(&r, &state, usize::from(call == Call::Release));
            }
            println!("CACHE_FAULT {operation:?} {call:?} count={count}");
        }
    }
}

#[test]
fn hash_and_equality_second_axis_window_keep_exact_work_and_cancel_prefix() {
    let values = axes(600);
    for hashing in [true, false] {
        for work_limit in [false, true] {
            let prefix = if hashing { 257 } else { 256 };
            let mut limits = Limits::default();
            if work_limit {
                limits.work = prefix;
            }
            let (r, state) = request(limits);
            if !work_limit {
                state.arm(Call::Checkpoint, prefix as usize + 1, cancelled());
            }
            let result = if hashing {
                key_hash(
                    &r,
                    &RandomState::new(),
                    fontdb::ID::dummy(),
                    GlyphId(7),
                    &values,
                )
                .map(|_| true)
            } else {
                axes_equal(&values, &values, &mut RequestControl(&r))
            };
            let expected = if work_limit {
                Error::new(FailureKind::Quota, LimitKind::Work)
            } else {
                cancelled()
            };
            assert_eq!(result.err(), Some(expected));
            assert_eq!(r.failure(), Some(expected));
            assert_eq!(
                r.statistics().work,
                if work_limit { prefix } else { prefix + 1 }
            );
            assert_eq!(
                state.counts().checkpoint,
                prefix as usize + usize::from(!work_limit)
            );
            state.assert_live(&r, 0);
            println!(
                "CACHE_AXIS_WINDOW hash={hashing} work={work_limit} completedAxes=256 error={expected:?}"
            );
        }
    }
    // A mismatch at the next item is reached only after the equal 256-item prefix.
    let mut next = values.clone();
    next[256].value = -123.0;
    let mut cx = markdown_svg_budget::NoControl::<std::convert::Infallible>::new();
    assert!(!axes_equal(&values, &next, &mut cx).unwrap());
}

#[test]
fn actual_lookup_hit_cached_none_and_miss_keep_every_typed_checkpoint_fault() {
    for shape in 0..3 {
        let builder = RandomState::new();
        let values = axes(600);
        let (r, state) = request(Limits::default());
        let mut cache = seeded_cache(&r, &builder);
        put(&mut cache, 7, &values, (shape == 0).then(outline));
        let glyph = if shape == 2 { 8 } else { 7 };
        let start = state.counts().checkpoint;
        let found = cache
            .lookup(fontdb::ID::dummy(), GlyphId(glyph), &values)
            .unwrap();
        let count = state.counts().checkpoint - start;
        assert_eq!(found.1.is_some(), shape != 2);
        drop(found);
        drop(cache);
        retire(&r, &state, 0);
        for ordinal in 1..=count {
            let (r, state) = request(Limits::default());
            let mut cache = seeded_cache(&r, &builder);
            put(&mut cache, 7, &values, (shape == 0).then(outline));
            state.arm(Call::Checkpoint, ordinal, cancelled());
            let before = state.counts().checkpoint;
            assert_eq!(
                cache
                    .lookup(fontdb::ID::dummy(), GlyphId(glyph), &values)
                    .err(),
                Some(cancelled())
            );
            assert_eq!(state.counts().checkpoint - before, ordinal);
            assert_eq!(r.failure(), Some(cancelled()));
            drop(cache);
            retire(&r, &state, 0);
        }
        println!("CACHE_LOOKUP shape={shape} checkpointFaults={count}");
    }
}

#[test]
fn actual_paid_hit_hash_and_equality_second_windows_use_original_request() {
    for equality in [false, true] {
        for quota in [false, true] {
            let values = axes(600);
            let (r, state) = request(Limits::default());
            let mut cache = OutlineCache::new(Some(&r));
            put(&mut cache, 7, &values, None);
            let event_start = state.lock().events.len();
            drop(
                cache
                    .lookup(fontdb::ID::dummy(), GlyphId(7), &values)
                    .unwrap(),
            );
            let checkpoints: Vec<_> = state.lock().events[event_start..]
                .iter()
                .filter(|e| e.call == Call::Checkpoint)
                .copied()
                .collect();
            assert_eq!(checkpoints[0].work, 0);
            assert_eq!(checkpoints[1].work, 1);
            let target = if equality {
                let group = checkpoints
                    .iter()
                    .position(|e| e.work == CTRL_GROUP_BYTES)
                    .unwrap();
                assert!(
                    checkpoints[group + 1..group + 602]
                        .iter()
                        .all(|e| e.work == 1)
                );
                group + 259
            } else {
                259
            };
            assert_eq!(checkpoints[target - 1].work, 1);
            let prefix: u64 = checkpoints[..target - 1].iter().map(|e| e.work).sum();
            let expected = if quota {
                Error::new(FailureKind::Quota, LimitKind::Work)
            } else {
                cancelled()
            };
            if quota {
                r.checkpoint(r.limits().work - r.statistics().work - prefix)
                    .unwrap();
            } else {
                state.arm(Call::Checkpoint, target, expected);
            }
            let before = state.counts().checkpoint;
            assert_eq!(
                cache.lookup(fontdb::ID::dummy(), GlyphId(7), &values).err(),
                Some(expected)
            );
            assert_eq!(r.failure(), Some(expected));
            assert_eq!(
                state.counts().checkpoint - before,
                target - usize::from(quota)
            );
            drop(cache);
            retire(&r, &state, 0);
            println!(
                "CACHE_REAL_WINDOW equality={equality} work={quota} completedAxes=256 checkpoint={target} prefixWork={prefix}"
            );
        }
    }
}

fn sparse_shared_table() -> (
    ResourcesRef,
    Arc<State>,
    Option<PaidShared<Table>>,
    PaidShared<Table>,
    RandomState,
) {
    let (r, state) = request(Limits::default());
    let hasher = RandomState::new();
    let glyph = (0..100_000)
        .map(GlyphId)
        .find(|glyph| {
            key_hash(&r, &hasher, fontdb::ID::dummy(), *glyph, &[]).unwrap() & 2047 >= 1024
        })
        .unwrap();
    let key = Key {
        font: fontdb::ID::dummy(),
        glyph,
        axes: copy_axes(&r, &[]).unwrap(),
        hash: key_hash(&r, &hasher, fontdb::ID::dummy(), glyph, &[]).unwrap(),
    };
    let mut map = Table::with_hasher_in(
        hasher.clone(),
        BucketAllocator::new(r.clone(), ResourceKind::Scratch),
    );
    map.try_reserve_with_control(1024, &mut RequestControl(&r), existing_hash)
        .unwrap();
    assert_eq!(
        map.capacity(),
        1792,
        "this test's fixed H table has 2048 control bytes"
    );
    map.try_insert_with_control(
        key.hash,
        key,
        None,
        &mut RequestControl(&r),
        keys_equal,
        existing_hash,
    )
    .unwrap();
    let owner = PaidShared::try_new(&r, ResourceKind::Scratch, map).unwrap();
    (r, state, Some(owner.clone()), owner, hasher)
}

#[test]
fn cow_scans_sparse_real_ctrl_groups_and_stops_at_second_256_byte_window() {
    let (r, state, mut table, alias, hasher) = sparse_shared_table();
    let start = state.counts().checkpoint;
    let event_start = state.lock().events.len();
    make_unique(&mut table, &r, &hasher).unwrap();
    let events = state.lock().events[event_start..].to_vec();
    let group = events
        .iter()
        .filter(|e| e.call == Call::Checkpoint && e.work == CTRL_GROUP_BYTES)
        .nth((256 / CTRL_GROUP_BYTES) as usize)
        .copied()
        .expect("control group after the first 256 control bytes, before the sparse entry");
    let relative_checkpoint = group.ordinal - start;
    let prior_work: u64 = events
        .iter()
        .filter(|e| e.call == Call::Checkpoint && e.ordinal < group.ordinal)
        .map(|e| e.work)
        .sum();
    let preceding_groups = events
        .iter()
        .filter(|e| {
            e.call == Call::Checkpoint && e.work == CTRL_GROUP_BYTES && e.ordinal < group.ordinal
        })
        .count();
    assert_eq!(
        preceding_groups as u64 * CTRL_GROUP_BYTES,
        256,
        "scan exactly 256 control bytes before rejecting the next group"
    );
    drop(table);
    drop(alias);
    retire(&r, &state, 0);
    for work in [false, true] {
        let (r, state, mut table, alias, hasher) = sparse_shared_table();
        let expected = if work {
            Error::new(FailureKind::Quota, LimitKind::Work)
        } else {
            cancelled()
        };
        // A fixed request limit cannot be changed after setup; consume its
        // remaining budget so exactly the baseline prefix can complete.
        if work {
            let available = r.limits().work - r.statistics().work;
            r.checkpoint(available - prior_work).unwrap();
        } else {
            state.arm(Call::Checkpoint, relative_checkpoint, expected);
        }
        let before = state.counts().checkpoint;
        assert_eq!(make_unique(&mut table, &r, &hasher).err(), Some(expected));
        assert!(std::ptr::eq(&**table.as_ref().unwrap(), &*alias));
        assert_eq!(
            state.counts().checkpoint - before,
            relative_checkpoint - usize::from(work)
        );
        assert_eq!(alias.len(), 1);
        drop(table);
        drop(alias);
        retire(&r, &state, 0);
        println!(
            "CACHE_COW_CTRL_WINDOW work={work} prefixControlBytes=256 checkpoint={relative_checkpoint} prefixWork={prior_work}"
        );
    }
}
