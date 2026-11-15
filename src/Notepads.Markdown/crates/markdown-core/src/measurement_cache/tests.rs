// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

fn cache() -> MeasurementCache {
    let owner = size_of::<MeasurementCache>();
    MeasurementCache::from_backing(vec![0; CACHE_TOTAL_BYTES - owner].into_boxed_slice(), owner)
        .unwrap()
}

fn allocate(bytes: usize) -> Option<Box<[u8]>> {
    Some(vec![0; bytes].into_boxed_slice())
}

#[test]
fn adaptive_cache_is_cold_on_misses_grows_bounded_and_clear_releases_capacity() {
    let owner = size_of::<MeasurementCache>();
    let cache =
        MeasurementCache::lazy(owner, |bytes| Some(vec![0xa5; bytes].into_boxed_slice())).unwrap();
    assert_eq!(cache.stats().allocated_bytes, owner as u64);
    assert_eq!(
        cache.lookup(1, b"unknown", &mut []).disposition,
        Disposition::Miss
    );
    assert_eq!(cache.stats().allocated_bytes, owner as u64);
    for index in 0u32..40 {
        let key = index.to_le_bytes();
        let value = vec![index as u8; 256 * 1024];
        assert_eq!(cache.put(1, &key, &value).disposition, Disposition::Stored);
        let mut copied = vec![0; value.len()];
        assert_eq!(
            cache.lookup(1, &key, &mut copied).disposition,
            Disposition::Hit
        );
        assert_eq!(copied, value);
        if index == 1 {
            // The second insertion grows the backing while retaining the first.
            assert!(cache.stats().allocated_bytes > 512 * 1024);
            assert_eq!(
                cache
                    .lookup(1, &0u32.to_le_bytes(), &mut copied)
                    .disposition,
                Disposition::Hit
            );
            assert!(copied.iter().all(|&byte| byte == 0));
        }
        assert!(cache.stats().allocated_bytes <= CACHE_TOTAL_BYTES as u64);
    }
    assert!(cache.stats().evictions > 0);
    let cleared = cache.clear();
    assert_eq!(cleared.allocated_bytes, owner as u64);
    assert_eq!(cleared.entries, 0);
    assert_eq!(
        cache.put(1, b"old", b"stale").disposition,
        Disposition::StaleEpoch
    );
    assert_eq!(
        cache.put(cleared.epoch, b"new", b"v").disposition,
        Disposition::Stored
    );
}

#[test]
fn poison_after_lazy_growth_can_release_storage_without_reusing_partial_entries() {
    let owner = size_of::<MeasurementCache>();
    let cache = MeasurementCache::lazy(owner, allocate).unwrap();
    cache.put(1, b"key", b"value");
    assert!(cache.stats().allocated_bytes > owner as u64);
    let _ = std::panic::catch_unwind(|| {
        let _guard = cache.state.lock().unwrap();
        panic!("optional cache fault");
    });
    let stats = cache.clear();
    assert!(!stats.enabled);
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.allocated_bytes, owner as u64);
    assert_eq!(
        cache.put(1, b"key", b"value").disposition,
        Disposition::Disabled
    );
}

#[test]
fn exact_capacity_and_invalid_capacity_are_independent_of_document_budget() {
    assert!(matches!(
        MeasurementCache::from_backing(vec![0; 10].into_boxed_slice(), 0),
        Err(CacheInitError::InvalidCapacity)
    ));
    let cache = cache();
    assert_eq!(cache.stats().allocated_bytes, CACHE_TOTAL_BYTES as u64);
    assert_eq!(cache.stats().epoch, 1);
    assert_eq!(cache.stats().entries, 0);
    assert!(cache.stats().enabled);
    assert_eq!(cache.clear().allocated_bytes, CACHE_TOTAL_BYTES as u64);
}

#[test]
fn hash_collision_requires_complete_bytes_and_duplicate_replaces_only_same_key() {
    let cache = cache();
    assert_eq!(
        cache.put_hashed(1, b"a\0x", b"first", 7).disposition,
        Disposition::Stored
    );
    cache.put_hashed(1, b"a\0y", b"other", 7);
    let mut output = [0; 8];
    assert_eq!(
        cache.lookup_hashed(1, b"a\0z", &mut output, 7).disposition,
        Disposition::Miss
    );
    assert_eq!(
        cache.lookup_hashed(1, b"a\0x", &mut output, 7).copied_bytes,
        5
    );
    assert_eq!(&output[..5], b"first");
    cache.put_hashed(1, b"a\0x", b"new", 7);
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(cache.stats().occupied_bytes, 3 + 3 + 3 + 5);
    // Replacement left an empty earlier slot and moved this key past the live count.
    assert_eq!(
        cache.lookup_hashed(1, b"a\0x", &mut output, 7).copied_bytes,
        3
    );
    assert_eq!(&output[..3], b"new");
    assert_eq!(
        cache.lookup_hashed(1, b"a\0y", &mut output, 7).copied_bytes,
        5
    );
    assert_eq!(&output[..5], b"other");
}

#[test]
fn query_has_no_pin_no_partial_copy_and_clear_rejects_late_writer() {
    let cache = cache();
    cache.put(1, b"key", b"payload");
    let mut short = [0xa5; 6];
    let query = cache.lookup(1, b"key", &mut short);
    assert_eq!(query.disposition, Disposition::NeedBuffer);
    assert_eq!(query.required_bytes, 7);
    assert_eq!(query.copied_bytes, 0);
    assert_eq!(short, [0xa5; 6]);
    assert_eq!(cache.stats().hits, 0);
    assert_eq!(cache.stats().probes, 1);
    let stats = cache.clear();
    assert_eq!(stats.epoch, 2);
    let mut output = [0xa5; 7];
    assert_eq!(
        cache.lookup(query.epoch, b"key", &mut output).disposition,
        Disposition::StaleEpoch
    );
    assert_eq!(
        cache.lookup(2, b"key", &mut output).disposition,
        Disposition::Miss
    );
    assert_eq!(
        cache.put(query.epoch, b"key", b"late").disposition,
        Disposition::StaleEpoch
    );
    assert_eq!(cache.stats().entries, 0);
    assert_eq!(output, [0xa5; 7]);
}

#[test]
fn second_lookup_checks_new_size_and_copies_current_complete_entry() {
    let cache = cache();
    cache.put(1, b"k", b"abc");
    assert_eq!(cache.lookup(1, b"k", &mut []).required_bytes, 3);
    cache.put(1, b"k", b"abcdef");
    let mut output = [0x55; 3];
    assert_eq!(
        cache.lookup(1, b"k", &mut output).disposition,
        Disposition::NeedBuffer
    );
    assert_eq!(output, [0x55; 3]);
    let mut output = [0x55; 8];
    let hit = cache.lookup(1, b"k", &mut output);
    assert_eq!(hit.required_bytes, hit.copied_bytes);
    assert_eq!(&output[..6], b"abcdef");
    assert_eq!(&output[6..], &[0x55; 2]);
}

#[test]
fn key_and_combined_entry_limits_reject_without_evicting() {
    let cache = cache();
    let key = vec![b'k'; KEY_LIMIT];
    let value = vec![b'v'; ENTRY_BYTES_LIMIT - KEY_LIMIT];
    assert_eq!(cache.put(1, &key, &value).disposition, Disposition::Stored);
    let before = cache.stats();
    assert_eq!(
        cache.put(1, &key, &vec![0; value.len() + 1]).disposition,
        Disposition::TooLarge
    );
    assert_eq!(
        cache.put(1, &vec![0; KEY_LIMIT + 1], b"x").disposition,
        Disposition::TooLarge
    );
    assert_eq!(cache.put(1, b"", b"x").disposition, Disposition::TooLarge);
    assert_eq!(cache.put(1, b"x", b"").disposition, Disposition::TooLarge);
    assert_eq!(cache.stats().entries, before.entries);
    assert_eq!(cache.stats().occupied_bytes, before.occupied_bytes);
    assert_eq!(cache.stats().evictions, 0);
    assert_eq!(
        cache.lookup(1, &key, &mut []).required_bytes as usize,
        value.len()
    );
}

#[test]
fn directory_capacity_evicts_one_slot_and_invalidates_a_query() {
    let cache = cache();
    for key in 0..ENTRY_LIMIT as u32 {
        cache.put(1, &key.to_le_bytes(), b"x");
    }
    assert_eq!(
        cache.lookup(1, &0_u32.to_le_bytes(), &mut []).disposition,
        Disposition::NeedBuffer
    );
    assert_eq!(cache.stats().entries as usize, ENTRY_LIMIT);
    cache.put(1, &(ENTRY_LIMIT as u32).to_le_bytes(), b"y");
    assert_eq!(cache.stats().entries as usize, ENTRY_LIMIT);
    assert_eq!(cache.stats().evictions, 1);
    assert_eq!(
        cache
            .lookup(1, &((ENTRY_LIMIT - 1) as u32).to_le_bytes(), &mut [0])
            .disposition,
        Disposition::Hit
    );
    assert_eq!(
        cache.lookup(1, &0_u32.to_le_bytes(), &mut [0]).disposition,
        Disposition::Miss
    );
    assert_eq!(
        cache.lookup(1, &1_u32.to_le_bytes(), &mut [0]).disposition,
        Disposition::Hit
    );
}

#[test]
fn ring_wrap_evicts_overlapping_entries_but_not_half_open_neighbors() {
    let cache = cache();
    cache.put(1, b"old1", &[1; 28]);
    cache.put(1, b"old2", &[2; 28]);
    {
        let mut state = cache.state.lock().unwrap();
        state.write_pos = state.backing.len() - DIRECTORY_BYTES - 1;
    }
    cache.put(1, b"new1", &[3; 28]);
    assert_eq!(
        cache.lookup(1, b"old1", &mut [0; 28]).disposition,
        Disposition::Miss
    );
    assert_eq!(
        cache.lookup(1, b"old2", &mut [0; 28]).disposition,
        Disposition::Hit
    );
    assert_eq!(cache.stats().evictions, 1);
    // This replacement starts at 32 and overlaps old2 only.
    cache.put(1, b"new2", &[4; 60]);
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(cache.stats().occupied_bytes, 32 + 64);
    assert_eq!(
        cache.lookup(1, b"old2", &mut [0; 28]).disposition,
        Disposition::Miss
    );
}

#[test]
fn repeated_natural_wraps_keep_live_payload_and_accounting_exact() {
    let cache = cache();
    for sequence in 0..60_u32 {
        let key = sequence.to_le_bytes();
        let value = vec![sequence as u8; ENTRY_BYTES_LIMIT - key.len()];
        cache.put(1, &key, &value);
        let state = cache.state.lock().unwrap();
        let mut count = 0;
        let mut occupied = 0;
        for slot in 0..ENTRY_LIMIT {
            if let Some(entry) = state.entry(slot) {
                count += 1;
                occupied += (entry.key + entry.value) as u64;
                let start = DIRECTORY_BYTES + entry.start;
                let value = state.backing[start];
                assert!(
                    state.backing[start + entry.key..start + entry.key + entry.value]
                        .iter()
                        .all(|&b| b == value)
                );
            }
        }
        assert_eq!(state.stats.entries, count);
        assert_eq!(state.stats.occupied_bytes, occupied);
        assert!(occupied as usize <= state.backing.len() - DIRECTORY_BYTES);
    }
    assert!(cache.stats().evictions > 0);
}

#[test]
fn epoch_exhaustion_disables_permanently_and_diagnostic_counters_saturate() {
    let cache = cache();
    {
        let mut state = cache.state.lock().unwrap();
        state.stats.epoch = u64::MAX;
        state.stats.puts = u64::MAX;
        state.stats.hits = u64::MAX;
        state.stats.rejected_puts = u64::MAX;
    }
    cache.put(u64::MAX, b"k", b"v");
    cache.lookup(u64::MAX, b"k", &mut [0]);
    assert_eq!(cache.stats().puts, u64::MAX);
    assert_eq!(cache.stats().hits, u64::MAX);
    assert!(!cache.clear().enabled);
    assert_eq!(
        cache.put(u64::MAX, b"k", b"v").disposition,
        Disposition::Disabled
    );
    assert_eq!(
        cache.lookup(u64::MAX, b"k", &mut [0]).disposition,
        Disposition::Disabled
    );
    assert!(!cache.clear().enabled);
    assert_eq!(cache.stats().epoch, u64::MAX);
    assert_eq!(cache.stats().entries, 0);
    assert_eq!(cache.stats().rejected_puts, u64::MAX);
}

#[test]
fn poisoned_mutex_fails_open_without_exposing_partial_entry() {
    let cache = cache();
    let _ = std::panic::catch_unwind(|| {
        let _guard = cache.state.lock().unwrap();
        panic!("cache failure injection");
    });
    assert!(!cache.stats().enabled);
    assert!(!cache.clear().enabled);
    assert_eq!(
        cache.lookup(1, b"k", &mut []).disposition,
        Disposition::Disabled
    );
    assert_eq!(cache.put(1, b"k", b"v").disposition, Disposition::Disabled);
}

#[test]
fn concurrent_clear_and_copy_are_atomic_and_old_epoch_put_cannot_repopulate() {
    let cache = cache();
    std::thread::scope(|scope| {
        for byte in 1..=4_u8 {
            let cache = &cache;
            scope.spawn(move || {
                let value = [byte; 1024];
                let mut output = [0; 1024];
                for _ in 0..40 {
                    cache.put(1, &[byte], &value);
                    if cache.lookup(1, &[byte], &mut output).disposition == Disposition::Hit {
                        assert_eq!(output, value);
                    }
                }
            });
        }
        cache.clear();
    });
    assert_eq!(cache.stats().epoch, 2);
    assert_eq!(cache.stats().entries, 0);
}
