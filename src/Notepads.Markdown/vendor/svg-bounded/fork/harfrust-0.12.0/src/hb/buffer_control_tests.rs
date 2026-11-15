// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private storage-core tests. They do not export or qualify controlled shaping.
use super::*;
use crate::hb::glyph_array::tests::support::{self, setup};
use std::sync::atomic::Ordering;

fn words<T: bytemuck::Pod>(values: &[T]) -> &[[u32; 5]] {
    bytemuck::cast_slice(values)
}
fn populated(paid: Option<&markdown_svg_budget::ResourcesRef>, length: usize) -> hb_buffer_t {
    let mut buffer = paid.map_or_else(hb_buffer_t::new, hb_buffer_t::new_for_storage_test);
    assert!(buffer.ensure(length));
    buffer.len = length;
    for i in 0..length {
        buffer.info[i] = support::info(i as u32);
    }
    buffer
}
fn same(a: &hb_buffer_t, b: &hb_buffer_t) {
    assert_eq!((a.idx, a.len, a.out_len), (b.idx, b.len, b.out_len));
    assert_eq!(
        (
            a.have_output,
            a.have_positions,
            a.have_separate_output,
            a.successful,
            a.max_ops
        ),
        (
            b.have_output,
            b.have_positions,
            b.have_separate_output,
            b.successful,
            b.max_ops
        )
    );
    assert_eq!(words(&a.info), words(&b.info));
    assert_eq!(words(&a.pos), words(&b.pos));
}

#[test]
fn real_output_expansion_rewind_sync_and_sort_keep_every_original_record_field() {
    let (r, state) = setup();
    let mut plain = populated(None, 600);
    let mut paid = populated(Some(&r), 600);
    for b in [&mut plain, &mut paid] {
        b.clear_output();
        b.next_glyphs(270);
        b.output_glyph(999);
        assert!(b.have_separate_output);
        b.next_glyphs(200);
        assert!(b.move_to(300));
        b.next_glyphs(20);
        assert!(b.sync());
        b.sort(0, b.len, |a, b| a.glyph_id > b.glyph_id);
    }
    same(&plain, &paid);
    assert!(paid.info.len() >= paid.len);
    // The inserted glyph makes sync grow both initialized extents to 601.
    assert_eq!(state.reserves.load(Ordering::Relaxed), 4);
    drop((plain, paid));
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

#[test]
fn sync_reinterprets_raw_storage_by_swapping_owners_and_initialized_extents() {
    let (r, state) = setup();
    let mut b = populated(Some(&r), 3);
    b.clear_output();
    b.output_glyph(555);
    b.next_glyphs(3);
    assert_eq!((b.info.len(), b.pos.len()), (4, 4));
    let (ip, pp) = (b.info.as_ptr() as usize, b.pos.as_ptr() as usize);
    let old_pos = words(&b.pos).to_vec();
    let old_info = words(&b.info).to_vec();
    let reservations = state.reserves.load(Ordering::Relaxed);
    assert!(b.sync());
    assert_eq!(
        (b.info.as_ptr() as usize, b.pos.as_ptr() as usize),
        (pp, ip)
    );
    assert_eq!(words(&b.info), &old_pos);
    assert_eq!(words(&b.pos), &old_info);
    assert_eq!(state.reserves.load(Ordering::Relaxed), reservations);
    drop(b);
    r.validate_retired().unwrap();
}

#[test]
fn each_real_info_position_reservation_and_commit_failure_prevents_candidate_length() {
    for commit in [false, true] {
        for fail in 1..=2 {
            let (r, state) = setup();
            if commit {
                state.fail_commit.store(fail, Ordering::Relaxed);
            } else {
                state.fail_reserve.store(fail, Ordering::Relaxed);
            }
            let mut b = hb_buffer_t::new_for_storage_test(&r);
            b.add(65, 0);
            assert!(!b.successful);
            assert_eq!((b.len, b.idx, b.out_len), (0, 0, 0));
            assert_eq!(
                r.failure(),
                Some(StorageError::native(if commit { -811 } else { -810 }))
            );
            assert_eq!(state.reserves.load(Ordering::Relaxed), fail);
            assert_eq!(
                state.commits.load(Ordering::Relaxed),
                if commit { fail } else { fail - 1 }
            );
            assert_eq!(b.info.len(), usize::from(fail == 2));
            assert_eq!(b.pos.len(), 0);
            assert!(!b.ensure(1)); // Initialized info alone cannot bypass the sticky error.
            b.clear();
            assert!(!b.successful); // Clearing cannot reset a failed paid request.
            drop(b);
            assert_eq!(state.live(), 0);
            r.validate_retired().unwrap();
        }
    }
}

#[test]
fn initialized_extent_is_not_spare_vec_capacity_in_either_storage_mode() {
    for paid in [false, true] {
        let (r, state) = setup();
        let mut b = populated(paid.then_some(&r), 8);
        b.clear();
        assert_eq!((b.info.len(), b.pos.len()), (0, 0));
        assert!(b.info.capacity() >= 8);
        assert!(b.ensure(3));
        assert_eq!((b.info.len(), b.pos.len()), (3, 3));
        assert!(words(&b.info).iter().all(|record| *record == [0; 5]));
        assert!(words(&b.pos).iter().all(|record| *record == [0; 5]));
        drop(b);
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn actual_capacity_length_overflow_and_operation_guards_record_typed_first_errors() {
    let (r, _) = setup();
    let mut b = hb_buffer_t::new_for_storage_test(&r);
    b.max_len = 1;
    b.add(65, 0);
    b.add(66, 1);
    assert_eq!(b.len, 1);
    assert!(!b.successful);
    assert_eq!(
        r.failure(),
        Some(StorageError::new(FailureKind::Quota, LimitKind::Capacity))
    );
    drop(b);
    r.validate_retired().unwrap();
    let (r, _) = setup();
    let mut b = hb_buffer_t::new_for_storage_test(&r);
    assert!(b.glyph_capacity_add(usize::MAX, 1).is_none());
    assert_eq!(
        r.failure(),
        Some(StorageError::new(FailureKind::Quota, LimitKind::Capacity))
    );
    drop(b);
    r.validate_retired().unwrap();
    let (r, _) = setup();
    let mut b = populated(Some(&r), 3);
    b.max_ops = 0;
    let before = words(&b.info).to_vec();
    b.set_masks(1, 1, 0, u32::MAX);
    assert_eq!(words(&b.info), &before);
    assert_eq!(
        r.failure(),
        Some(StorageError::new(FailureKind::Quota, LimitKind::Work))
    );
    assert!(!b.successful);
    drop(b);
    r.validate_retired().unwrap();
}

#[test]
fn ordinary_operation_failure_keeps_the_original_flag_and_void_continuation() {
    let mut b = populated(None, 3);
    b.max_ops = 0;
    b.set_masks(0x40, 0x40, 0, u32::MAX);
    assert!(!b.successful);
    assert_eq!(b.max_ops, -3);
    assert!(b.info.iter().all(|info| info.mask & 0x40 != 0));
}

#[test]
fn second_window_failure_during_output_separation_preserves_source_and_active_state() {
    let (r, state) = setup();
    let mut b = populated(Some(&r), 600);
    b.clear_output();
    b.next_glyphs(513);
    let source = words(&b.info).to_vec();
    state.reset_windows(2);
    b.output_glyph(999);
    assert!(!b.successful);
    assert_eq!(
        (b.len, b.idx, b.out_len, b.have_separate_output),
        (600, 513, 513, false)
    );
    assert_eq!(words(&b.info), &source);
    assert_eq!(words(&b.pos[..256]), &source[..256]);
    assert!(words(&b.pos[256..]).iter().all(|value| *value == [0; 5]));
    assert_eq!(state.windows.load(Ordering::Relaxed), 2);
    drop(b);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

#[test]
fn shift_gap_and_overlap_match_original_and_failed_move_never_commits_new_counts() {
    for (length, index, count) in [(3, 1, 8), (600, 5, 7), (600, 500, 100)] {
        let (r, _) = setup();
        let mut plain = populated(None, length);
        let mut paid = populated(Some(&r), length);
        for b in [&mut plain, &mut paid] {
            b.clear_output();
            b.idx = index;
            assert!(b.shift_forward(count));
        }
        same(&plain, &paid);
        drop((plain, paid));
        r.validate_retired().unwrap();
    }
    let (r, state) = setup();
    let mut b = populated(Some(&r), 605);
    b.clear_output();
    b.len = 600;
    state.reset_windows(2);
    assert!(!b.shift_forward(5));
    assert_eq!((b.len, b.idx), (600, 0));
    assert_eq!(state.windows.load(Ordering::Relaxed), 2);
    for i in 349..605 {
        assert_eq!(b.info[i].glyph_id, (i - 5) as u32);
    }
    for i in 0..349 {
        assert_eq!(b.info[i].glyph_id, i as u32);
    }
    drop(b);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

#[test]
fn growth_failure_preserves_existing_glyphs_and_stops_before_the_next_array() {
    for commit in [false, true] {
        for which in 1..=2 {
            let (r, state) = setup();
            let mut b = populated(Some(&r), 8);
            let old_info = words(&b.info).to_vec();
            let old_pos = words(&b.pos).to_vec();
            if commit {
                state.fail_commit.store(2 + which, Ordering::Relaxed);
            } else {
                state.fail_reserve.store(2 + which, Ordering::Relaxed);
            }
            assert!(!b.ensure(16));
            assert_eq!((b.len, b.idx, b.out_len), (8, 0, 0));
            assert_eq!(&words(&b.info)[..8], &old_info);
            assert_eq!(&words(&b.pos)[..8], &old_pos);
            assert_eq!(b.info.len(), if which == 1 { 8 } else { 16 });
            assert_eq!(b.pos.len(), 8);
            assert_eq!(state.reserves.load(Ordering::Relaxed), 2 + which);
            assert_eq!(
                r.failure(),
                Some(StorageError::native(if commit { -811 } else { -810 }))
            );
            drop(b);
            assert_eq!(state.live(), 0);
            r.validate_retired().unwrap();
        }
    }
}

#[test]
fn natural_old_info_retirement_failure_rejects_growth_before_position_reservation() {
    let (r, state) = setup();
    let mut b = populated(Some(&r), 8);
    let source = words(&b.info).to_vec();
    state.fail_release.store(1, Ordering::Relaxed);
    assert!(!b.ensure(16));
    assert_eq!((b.len, b.info.len(), b.pos.len()), (8, 8, 8));
    assert_eq!((b.info.capacity(), b.pos.capacity()), (16, 8));
    assert_eq!(words(&b.info), &source);
    assert_eq!(r.failure(), Some(StorageError::native(-812)));
    assert_eq!(state.reserves.load(Ordering::Relaxed), 3);
    assert_eq!(state.commits.load(Ordering::Relaxed), 3);
    assert!(!b.ensure(16));
    assert_eq!(state.reserves.load(Ordering::Relaxed), 3);
    drop(b);
    assert_eq!(state.releases.load(Ordering::Relaxed), 3);
    assert_eq!(state.live(), 8 * 20); // Exactly the failed old-block refund remains.
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(r.validate_retired(), Err(StorageError::native(-812)));
}

#[test]
fn rewind_cancellation_and_position_zeroing_do_not_publish_new_buffer_flags_or_counts() {
    let (r, state) = setup();
    let mut b = populated(Some(&r), 600);
    b.clear_output();
    b.next_glyphs(600);
    state.reset_windows(2);
    assert!(!b.move_to(0));
    assert_eq!((b.idx, b.out_len), (600, 600));
    assert_eq!(state.windows.load(Ordering::Relaxed), 2);
    drop(b);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();

    let (r, state) = setup();
    let mut b = populated(Some(&r), 600);
    b.clear_output();
    for pos in &mut b.pos {
        *pos = bytemuck::cast([1u32; 5]);
    }
    state.reset_windows(2);
    b.clear_positions();
    assert!(!b.successful);
    assert!(b.have_output);
    assert!(!b.have_positions);
    assert!(words(&b.pos[..256]).iter().all(|record| *record == [0; 5]));
    assert!(words(&b.pos[256..]).iter().all(|record| *record == [1; 5]));
    drop(b);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}
