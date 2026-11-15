// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;

pub(crate) mod support {
    use markdown_svg_budget::{
        Backend, Error, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    #[derive(Default)]
    pub(crate) struct State {
        pub reserves: AtomicUsize,
        pub commits: AtomicUsize,
        pub releases: AtomicUsize,
        pub windows: AtomicUsize,
        pub fail_reserve: AtomicUsize,
        pub fail_commit: AtomicUsize,
        pub fail_window: AtomicUsize,
        pub fail_release: AtomicUsize,
        pub entries: Mutex<std::vec::Vec<(u32, usize)>>,
    }
    impl State {
        pub fn live(&self) -> usize {
            self.entries.lock().unwrap().iter().map(|(_, n)| n).sum()
        }
        pub fn reset_windows(&self, fail: usize) {
            self.windows.store(0, Ordering::Relaxed);
            self.fail_window.store(fail, Ordering::Relaxed);
        }
    }
    struct Host(Arc<State>);
    impl Backend for Host {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            let call = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_reserve.load(Ordering::Relaxed) {
                return Err(Error::native(-810));
            }
            self.0.entries.lock().unwrap().push((call as u32, bytes));
            Ticket::new(1, call as u32, 1)
        }
        fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
            assert!(self
                .0
                .entries
                .lock()
                .unwrap()
                .contains(&(ticket.slot(), bytes)));
            let call = self.0.commits.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_commit.load(Ordering::Relaxed) {
                return Err(Error::native(-811));
            }
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            let call = self.0.releases.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_release.load(Ordering::Relaxed) {
                return Err(Error::native(-812));
            }
            let mut entries = self.0.entries.lock().unwrap();
            let index = entries
                .iter()
                .position(|(id, _)| *id == ticket.slot())
                .unwrap();
            entries.swap_remove(index);
            Ok(())
        }
        fn checkpoint(&self, work: u64) -> Result<(), Error> {
            if work != 0 {
                let call = self.0.windows.fetch_add(1, Ordering::Relaxed) + 1;
                if call == self.0.fail_window.load(Ordering::Relaxed) {
                    return Err(Error::new(
                        markdown_svg_budget::FailureKind::Cancelled,
                        markdown_svg_budget::LimitKind::Work,
                    ));
                }
            }
            Ok(())
        }
    }
    pub(crate) fn setup() -> (ResourcesRef, Arc<State>) {
        setup_with_limits(Limits::default())
    }
    pub(crate) fn setup_with_limits(limits: Limits) -> (ResourcesRef, Arc<State>) {
        let state = Arc::new(State {
            entries: Mutex::new(std::vec::Vec::with_capacity(64)),
            ..State::default()
        });
        let resources =
            ResourcesRef::new_prepaid(Host(state.clone()), FirstFailureRef::new_prepaid(), limits);
        (resources, state)
    }
    pub(crate) fn info(n: u32) -> super::GlyphInfo {
        bytemuck::cast([
            n,
            n ^ 0x8000_0000,
            n.wrapping_mul(17),
            !n,
            n.rotate_left(13),
        ])
    }
}
use std::sync::atomic::Ordering;
use support::*;

#[test]
fn sealed_views_swap_the_complete_paid_block_without_copy_or_admission() {
    let (r, state) = setup();
    let mut info = GlyphArray::<GlyphInfo>::new(Some(&r));
    let mut pos = GlyphArray::<GlyphPosition>::new(Some(&r));
    info.try_resize(513, support::info(7)).unwrap();
    pos.try_resize(
        257,
        bytemuck::cast([0x8000_0000u32, 0xffff_ffff, 0x7fc0_0001, 0, 42]),
    )
    .unwrap();
    let (ip, pp) = (
        info.as_slice().as_ptr() as usize,
        pos.as_slice().as_ptr() as usize,
    );
    let (ic, pc) = (info.capacity(), pos.capacity());
    let live = state.live();
    let reserves = state.reserves.load(Ordering::Relaxed);
    info.swap_storage(&mut pos).unwrap();
    assert_eq!((info.len(), pos.len()), (257, 513));
    assert_eq!((info.capacity(), pos.capacity()), (pc, ic));
    assert_eq!(
        (
            info.as_slice().as_ptr() as usize,
            pos.as_slice().as_ptr() as usize
        ),
        (pp, ip)
    );
    assert_eq!(
        bytemuck::cast::<_, Words>(info[0]),
        [0x8000_0000, 0xffff_ffff, 0x7fc0_0001, 0, 42]
    );
    assert_eq!(
        bytemuck::cast::<_, Words>(pos[512]),
        bytemuck::cast::<_, Words>(support::info(7))
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), reserves);
    assert_eq!(state.live(), live);
    drop(info);
    assert_eq!(state.live(), ic * 20);
    drop(pos);
    assert_eq!(state.live(), 0);
    r.validate_retired().unwrap();
}

#[test]
fn clearing_initialized_records_keeps_capacity_paid_until_last_owner_drops() {
    let (r, state) = setup();
    let mut info = GlyphArray::<GlyphInfo>::new(Some(&r));
    info.try_resize(600, support::info(1)).unwrap();
    let paid = state.live();
    info.clear();
    assert_eq!(info.len(), 0);
    assert_eq!(info.capacity() * 20, paid);
    assert_eq!(state.live(), paid);
    info.try_resize(400, support::info(2)).unwrap();
    assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
    assert_eq!(info[399].glyph_id, 2);
    drop(info);
    r.validate_retired().unwrap();
    assert_eq!(state.live(), 0);
}

#[test]
fn overlap_copy_matches_memmove_in_both_directions_and_on_zero_length() {
    for (source, destination) in [(0..600, 5), (5..605, 0), (0..0, 605), (3..604, 3)] {
        for paid in [false, true] {
            let (r, state) = setup();
            let mut array = GlyphArray::new(paid.then_some(&r));
            array.try_resize(605, support::info(0)).unwrap();
            for (i, value) in array.iter_mut().enumerate() {
                *value = support::info(i as u32);
            }
            let mut expected = array.as_slice().to_vec();
            expected.copy_within(source.clone(), destination);
            array.try_copy_within(source.clone(), destination).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<_, Words>(&array),
                bytemuck::cast_slice::<_, Words>(&expected)
            );
            drop(array);
            r.validate_retired().unwrap();
            assert_eq!(state.live(), 0);
        }
    }
}

#[test]
fn second_copy_window_failure_stops_before_the_rejected_prefix_or_suffix() {
    for (source, destination, changed, unchanged) in
        [(0..600, 5, 349..605, 5..349), (5..605, 0, 0..256, 256..600)]
    {
        let (r, state) = setup();
        let mut array = GlyphArray::new(Some(&r));
        array.try_resize(605, support::info(0)).unwrap();
        for (i, value) in array.iter_mut().enumerate() {
            *value = support::info(i as u32);
        }
        state.reset_windows(2);
        let error = array
            .try_copy_within(source.clone(), destination)
            .unwrap_err();
        assert_eq!(error, Error::new(FailureKind::Cancelled, LimitKind::Work));
        assert_eq!(r.failure(), Some(error));
        assert_eq!(state.windows.load(Ordering::Relaxed), 2);
        for i in changed {
            assert_eq!(array[i].glyph_id, (source.start + i - destination) as u32);
        }
        for i in unchanged {
            assert_eq!(array[i].glyph_id, i as u32);
        }
        drop(array);
        assert_eq!(state.live(), 0);
        r.validate_retired().unwrap(); // Retirement and the sticky failure are separate contracts.
        assert_eq!(r.failure(), Some(error));
    }
}

#[test]
fn resize_and_fill_cancel_before_the_second_256_record_window() {
    let (r, state) = setup();
    let mut array = GlyphArray::<GlyphInfo>::new(Some(&r));
    state.reset_windows(2);
    let error = array.try_resize(600, support::info(9)).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(array.len(), 256);
    assert!(array.iter().all(|value| value.glyph_id == 9));
    assert_eq!(state.live(), array.capacity() * 20);
    drop(array);
    assert_eq!(state.live(), 0);
    let (r, state) = setup();
    let mut array = GlyphArray::<GlyphInfo>::new(Some(&r));
    array.try_resize(600, support::info(1)).unwrap();
    state.reset_windows(2);
    assert_eq!(
        array.try_fill(0..600, support::info(2)).unwrap_err().kind,
        FailureKind::Cancelled
    );
    assert!(array[..256].iter().all(|value| value.glyph_id == 2));
    assert!(array[256..].iter().all(|value| value.glyph_id == 1));
    drop(array);
    assert_eq!(state.live(), 0);
}

#[test]
fn foreign_request_and_plain_paid_swaps_reject_before_either_owner_moves() {
    for foreign in [false, true] {
        let (r, state) = setup();
        let (other_r, other_state) = setup();
        let mut info = GlyphArray::new(Some(&r));
        let mut pos = GlyphArray::<GlyphPosition>::new(foreign.then_some(&other_r));
        info.try_resize(1, support::info(1)).unwrap();
        pos.try_resize(2, GlyphPosition::default()).unwrap();
        let (ip, pp) = (info.as_ptr(), pos.as_ptr());
        assert_eq!(
            info.swap_storage(&mut pos),
            Err(Error::new(FailureKind::Invariant, LimitKind::None))
        );
        assert_eq!((info.as_ptr(), pos.as_ptr()), (ip, pp));
        assert_eq!((info.len(), pos.len()), (1, 2));
        drop((info, pos));
        assert_eq!((state.live(), other_state.live()), (0, 0));
        assert!(other_r.failure().is_none());
    }
}

#[test]
fn release_failure_is_original_typed_error_and_never_replaced_by_later_work() {
    let (r, state) = setup();
    let mut array = GlyphArray::new(Some(&r));
    array.try_resize(4, support::info(1)).unwrap();
    state.fail_release.store(1, Ordering::Relaxed);
    drop(array);
    let original = Error::native(-812);
    assert_eq!(r.failure(), Some(original));
    assert_eq!(r.checkpoint(1), Err(original));
    assert_eq!(r.validate_retired(), Err(original));
    assert_eq!(state.releases.load(Ordering::Relaxed), 1);
    assert_eq!(state.live(), 80); // Failed host refund is retained, never falsely retired.
}
