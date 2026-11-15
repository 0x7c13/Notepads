// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
extern crate std;

use super::*;
use crate::{Family, Language, Stretch, Style, Weight, ID};
use alloc::{sync::Arc, vec};

fn owner() -> Arc<dyn AsRef<[u8]> + Send + Sync> {
    Arc::new(vec![3u8, 7, 11, 19])
}

fn original_face(source: Arc<dyn AsRef<[u8]> + Send + Sync>, index: u32) -> FaceInfo {
    FaceInfo {
        id: ID::dummy(),
        source: Source::Binary(source),
        index,
        families: vec![
            ("Family-α".into(), Language::English_UnitedStates),
            ("其他族名".into(), Language::English_UnitedStates),
        ],
        post_script_name: "OriginalPS".into(),
        style: Style::Italic,
        weight: Weight::BOLD,
        stretch: Stretch::Expanded,
        monospaced: true,
    }
}

fn compare_face(first: &FaceInfo, second: &FaceInfo) {
    assert_eq!(first.id, second.id);
    assert_eq!(first.index, second.index);
    assert_eq!(first.families, second.families);
    assert_eq!(first.post_script_name, second.post_script_name);
    assert_eq!(first.style, second.style);
    assert_eq!(first.weight, second.weight);
    assert_eq!(first.stretch, second.stretch);
    assert_eq!(first.monospaced, second.monospaced);
    let (Source::Binary(first), Source::Binary(second)) = (&first.source, &second.source) else {
        panic!("test expects Binary source owners");
    };
    assert!(Arc::ptr_eq(first, second));
}

#[test]
fn explicit_compat_clone_preserves_full_metadata_keys_and_free_list() {
    let source = owner();
    let mut old = Database::new();
    old.set_serif_family("Custom中文");
    let first = old.push_face_info(original_face(source.clone(), 2));
    let removed = old.push_face_info(original_face(source.clone(), 3));
    let last = old.push_face_info(original_face(source.clone(), 5));
    old.remove_face(removed);
    let mut next = old.try_clone_unfunded().unwrap();
    assert!(!next.is_funded());
    assert_eq!(next.family_name(&Family::Serif), "Custom中文");
    compare_face(old.face(first).unwrap(), next.face(first).unwrap());
    compare_face(old.face(last).unwrap(), next.face(last).unwrap());
    assert!(next.face(removed).is_none());
    let expected = old.push_face_info(original_face(source.clone(), 7));
    let actual = next.push_face_info(original_face(source, 7));
    assert_eq!(actual, expected);
    assert_ne!(actual, removed);
}

#[cfg(feature = "fs")]
#[test]
fn compat_source_paths_are_copied_without_lossy_utf8() {
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        std::path::PathBuf::from(std::ffi::OsString::from_wide(&[0xd800, 65, 0xdc00]))
    };
    #[cfg(not(windows))]
    let path = std::path::PathBuf::from("/tmp/é-font.ttf");
    let mut db = Database::new();
    let mut info = original_face(owner(), 3);
    info.source = Source::File(path.clone());
    let id = db.push_face_info(info);
    let copied = db.try_clone_unfunded().unwrap();
    let Source::File(copied_path) = &copied.face(id).unwrap().source else {
        panic!("source variant changed");
    };
    assert_eq!(copied_path.as_os_str(), path.as_os_str());
}

#[cfg(feature = "notepads-markdown-resources")]
mod controlled {
    use super::*;
    use core::{alloc::Layout, cell::Cell, ptr};
    use markdown_svg_budget::{Backend, FirstFailureRef, Limits, Ticket};
    use std::{
        alloc::{GlobalAlloc, System},
        sync::Mutex,
    };

    const MAX_EVENTS: usize = 4096;
    #[derive(Clone, Copy, Debug)]
    struct Event {
        tag: u8,
        identity: usize,
        bytes: usize,
        alignment: usize,
    }
    const EMPTY: Event = Event {
        tag: 0,
        identity: 0,
        bytes: 0,
        alignment: 0,
    };
    struct Trace {
        events: [Event; MAX_EVENTS],
        len: usize,
        allocations: usize,
        fail_allocation: usize,
        overflowed: bool,
    }
    impl Trace {
        fn new(fail_allocation: usize) -> Self {
            Self {
                events: [EMPTY; MAX_EVENTS],
                len: 0,
                allocations: 0,
                fail_allocation,
                overflowed: false,
            }
        }
        fn record(&mut self, event: Event) {
            if self.len == self.events.len() {
                self.overflowed = true;
            } else {
                self.events[self.len] = event;
                self.len += 1;
            }
        }
    }
    std::thread_local! {
        // Allocator callbacks only touch an already initialized, stack-owned
        // fixed array. Other tests and libtest threads are never observed.
        static TRACE: Cell<*mut Trace> = const { Cell::new(ptr::null_mut()) };
    }
    fn with_trace<R>(call: impl FnOnce(&mut Trace) -> R) -> Option<R> {
        TRACE
            .try_with(|slot| {
                let trace = slot.get();
                if trace.is_null() {
                    None
                } else {
                    // SAFETY: the observing closure has exclusive access to this
                    // TLS-selected Trace and allocator callbacks never allocate.
                    Some(call(unsafe { &mut *trace }))
                }
            })
            .ok()
            .flatten()
    }
    fn event(tag: u8, identity: usize, bytes: usize, alignment: usize) {
        with_trace(|trace| {
            trace.record(Event {
                tag,
                identity,
                bytes,
                alignment,
            })
        });
    }
    fn reject_allocation(layout: Layout) -> bool {
        with_trace(|trace| {
            trace.allocations += 1;
            if trace.allocations == trace.fail_allocation {
                trace.record(Event {
                    tag: 6,
                    identity: 0,
                    bytes: layout.size(),
                    alignment: layout.align(),
                });
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
    }
    struct Observer;
    unsafe impl GlobalAlloc for Observer {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if reject_allocation(layout) {
                return ptr::null_mut();
            }
            let value = unsafe { System.alloc(layout) };
            event(1, value as usize, layout.size(), layout.align());
            value
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if reject_allocation(layout) {
                return ptr::null_mut();
            }
            let value = unsafe { System.alloc_zeroed(layout) };
            event(1, value as usize, layout.size(), layout.align());
            value
        }
        unsafe fn realloc(&self, value: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let next_layout = Layout::from_size_align(size, layout.align()).unwrap();
            if reject_allocation(next_layout) {
                return ptr::null_mut();
            }
            let next = unsafe { System.realloc(value, layout, size) };
            if !next.is_null() {
                event(2, value as usize, layout.size(), layout.align());
                event(1, next as usize, size, layout.align());
            }
            next
        }
        unsafe fn dealloc(&self, value: *mut u8, layout: Layout) {
            unsafe {
                System.dealloc(value, layout);
            }
            event(2, value as usize, layout.size(), layout.align());
        }
    }
    #[global_allocator]
    static OBSERVER: Observer = Observer;

    fn observing<R>(trace: &mut Trace, call: impl FnOnce() -> R) -> R {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                TRACE.with(|slot| slot.set(ptr::null_mut()));
            }
        }
        TRACE.with(|slot| {
            assert!(slot.get().is_null());
            slot.set(trace);
        });
        let _guard = Guard;
        call()
    }

    #[derive(Clone, Copy, Default)]
    struct Entry {
        bytes: usize,
        admitted: bool,
    }
    struct State {
        entries: [Entry; 512],
        reserves: usize,
        admissions: usize,
        checkpoints: usize,
        live: usize,
        peak: usize,
        fail_reserve: usize,
        fail_admission: usize,
        fail_checkpoint: usize,
        fail_release: bool,
    }
    impl Default for State {
        fn default() -> Self {
            Self {
                entries: [Entry::default(); 512],
                reserves: 0,
                admissions: 0,
                checkpoints: 0,
                live: 0,
                peak: 0,
                fail_reserve: 0,
                fail_admission: 0,
                fail_checkpoint: 0,
                fail_release: false,
            }
        }
    }
    struct TestBackend(Arc<Mutex<State>>);
    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            let mut state = self.0.lock().unwrap();
            state.reserves += 1;
            if state.reserves == state.fail_reserve {
                return Err(Error::new(FailureKind::Quota, LimitKind::Bytes));
            }
            let slot = state.reserves;
            state.entries[slot] = Entry {
                bytes,
                admitted: false,
            };
            state.live += bytes;
            state.peak = state.peak.max(state.live);
            event(3, slot, bytes, 0);
            Ticket::new(1, slot as u32, 1)
        }
        fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
            let mut state = self.0.lock().unwrap();
            state.admissions += 1;
            if state.admissions == state.fail_admission {
                event(7, ticket.slot() as usize, bytes, 0);
                return Err(Error::native(-778));
            }
            let entry = &mut state.entries[ticket.slot() as usize];
            assert_eq!(entry.bytes, bytes);
            assert!(!entry.admitted);
            entry.admitted = true;
            event(4, ticket.slot() as usize, bytes, 0);
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            let mut state = self.0.lock().unwrap();
            if state.fail_release {
                return Err(Error::native(-777));
            }
            let entry = core::mem::take(&mut state.entries[ticket.slot() as usize]);
            assert_ne!(entry.bytes, 0);
            state.live -= entry.bytes;
            event(5, ticket.slot() as usize, entry.bytes, 0);
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            let mut state = self.0.lock().unwrap();
            state.checkpoints += 1;
            if state.checkpoints == state.fail_checkpoint {
                return Err(Error::new(FailureKind::Cancelled, LimitKind::None));
            }
            Ok(())
        }
    }
    fn setup() -> (ResourcesRef, Arc<Mutex<State>>) {
        let state = Arc::new(Mutex::new(State::default()));
        let resources = ResourcesRef::new_prepaid(
            TestBackend(state.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        (resources, state)
    }
    fn base() -> (Database, ID, ID, Arc<dyn AsRef<[u8]> + Send + Sync>) {
        let source = owner();
        let mut db = Database::new();
        let first = db.push_face_info(original_face(source.clone(), 2));
        let removed = db.push_face_info(original_face(source.clone(), 3));
        db.push_face_info(original_face(source.clone(), 5));
        db.remove_face(removed);
        (db, first, removed, source)
    }
    fn build(
        db: &Database,
        resources: &ResourcesRef,
        source: &Arc<dyn AsRef<[u8]> + Send + Sync>,
    ) -> Result<Database, Error> {
        db.try_extend_in(
            resources,
            2,
            |index| {
                Ok(FaceRegistration {
                    source: source.clone(),
                    index: 17 + index as u32,
                    family: if index == 0 {
                        "actual-face-1-0"
                    } else {
                        "actual-face-2-1"
                    },
                })
            },
            &mut [ID::dummy(); 2],
        )
    }
    fn assert_orders(trace: &Trace, require_all_allocations_paid: bool) {
        assert!(!trace.overflowed);
        let events = &trace.events[..trace.len];
        for (at, reserved) in events
            .iter()
            .enumerate()
            .filter(|(_, event)| event.tag == 3)
        {
            let released = events
                .iter()
                .enumerate()
                .skip(at + 1)
                .find(|(_, event)| event.tag == 5 && event.identity == reserved.identity)
                .unwrap();
            let allocation = events
                .iter()
                .enumerate()
                .skip(at + 1)
                .take(released.0 - at - 1)
                .find(|(_, event)| {
                    (event.tag == 1 || event.tag == 6) && event.bytes == reserved.bytes
                });
            let Some((allocated_at, allocated)) = allocation else {
                continue;
            };
            if allocated.tag == 6 {
                continue;
            }
            assert_ne!(allocated.identity, 0);
            let admission = events
                .iter()
                .enumerate()
                .skip(allocated_at + 1)
                .take(released.0 - allocated_at - 1)
                .find(|(_, event)| {
                    (event.tag == 4 || event.tag == 7) && event.identity == reserved.identity
                })
                .unwrap();
            let freed = events
                .iter()
                .enumerate()
                .skip(admission.0 + 1)
                .take(released.0 - admission.0 - 1)
                .find(|(_, event)| event.tag == 2 && event.identity == allocated.identity)
                .unwrap();
            assert!(
                at < allocated_at
                    && allocated_at < admission.0
                    && admission.0 < freed.0
                    && freed.0 < released.0
            );
            assert_eq!(allocated.bytes, freed.1.bytes);
            assert_eq!(allocated.alignment, freed.1.alignment);
        }
        if require_all_allocations_paid {
            for (at, allocated) in events
                .iter()
                .enumerate()
                .filter(|(_, event)| event.tag == 1)
            {
                let reserved = events[..at]
                    .iter()
                    .rposition(|event| event.tag == 3 && event.bytes == allocated.bytes)
                    .unwrap();
                assert!(events[reserved + 1..at].iter().all(|event| event.tag != 1));
            }
        }
    }

    #[test]
    fn paid_versions_keep_metadata_ids_holes_and_source_owners_with_real_free_order() {
        let (old, first, removed, source) = base();
        let old = Arc::new(old);
        let (resources, state) = setup();
        let mut trace = Trace::new(0);
        let mut ids = [ID::dummy(); 2];
        observing(&mut trace, || {
            let next = old
                .try_extend_in(
                    &resources,
                    2,
                    |index| {
                        Ok(FaceRegistration {
                            source: source.clone(),
                            index: 17 + index as u32,
                            family: "actual-face",
                        })
                    },
                    &mut ids,
                )
                .unwrap();
            compare_face(old.face(first).unwrap(), next.face(first).unwrap());
            assert_eq!(old.len(), 2);
            assert_eq!(next.len(), 4);
            assert!(old.face(ids[0]).is_none());
            assert!(old.face(ids[1]).is_none());
            assert!(next.face(removed).is_none());
            let expected = {
                let mut plain = old.try_clone_unfunded().unwrap();
                plain.push_face_info(original_face(source.clone(), 17))
            };
            assert_eq!(ids[0], expected);
            assert_eq!(next.face(ids[0]).unwrap().index, 17);
            assert_eq!(next.face(ids[1]).unwrap().index, 18);
            assert!(next.is_funded());
            assert_eq!(
                next.try_clone_unfunded().unwrap_err(),
                CompatCloneError::ControlledReadOnly
            );
            drop(next);
        });
        assert_eq!(state.lock().unwrap().live, 0);
        assert_orders(&trace, false); // The explicit compatibility clone is intentionally unfunded.
        resources.validate_retired().unwrap();
    }

    #[test]
    fn every_new_allocation_is_prepaid_and_each_reserve_admit_cancel_failure_rolls_back() {
        let (old, first, removed, source) = base();
        let (resources, state) = setup();
        let mut trace = Trace::new(0);
        observing(&mut trace, || {
            drop(build(&old, &resources, &source).unwrap())
        });
        assert_orders(&trace, true);
        let baseline = state.lock().unwrap();
        let (reserves, admissions, checkpoints) =
            (baseline.reserves, baseline.admissions, baseline.checkpoints);
        drop(baseline);
        for category in 0..3 {
            let total = [reserves, admissions, checkpoints][category];
            for fail_at in 1..=total {
                let (resources, state) = setup();
                {
                    let mut state = state.lock().unwrap();
                    match category {
                        0 => state.fail_reserve = fail_at,
                        1 => state.fail_admission = fail_at,
                        _ => state.fail_checkpoint = fail_at,
                    }
                }
                let mut trace = Trace::new(0);
                let error = observing(&mut trace, || build(&old, &resources, &source).unwrap_err());
                assert_eq!(error, resources.failure().unwrap());
                assert_eq!(
                    error.kind,
                    [
                        FailureKind::Quota,
                        FailureKind::Native,
                        FailureKind::Cancelled
                    ][category]
                );
                assert_eq!(state.lock().unwrap().live, 0);
                assert_eq!(old.len(), 2);
                assert!(old.face(first).is_some());
                assert!(old.face(removed).is_none());
                assert_orders(&trace, true);
                resources.validate_retired().unwrap();
            }
        }
    }

    #[test]
    fn actual_allocator_null_at_every_storage_allocation_frees_before_refund() {
        let (old, _, _, source) = base();
        let (resources, _) = setup();
        let mut baseline = Trace::new(0);
        observing(&mut baseline, || {
            drop(build(&old, &resources, &source).unwrap())
        });
        for fail_at in 1..=baseline.allocations {
            let (resources, state) = setup();
            let mut trace = Trace::new(fail_at);
            let error = observing(&mut trace, || build(&old, &resources, &source).unwrap_err());
            assert_eq!(error.kind, FailureKind::Allocation);
            assert_eq!(error, resources.failure().unwrap());
            assert_eq!(state.lock().unwrap().live, 0);
            assert_orders(&trace, true);
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn factory_error_and_unwind_retire_partial_new_faces_without_changing_old_version() {
        let (old, first, _, source) = base();
        for unwind in [false, true] {
            let (resources, state) = setup();
            let payload: std::boxed::Box<dyn core::any::Any + Send> = std::boxed::Box::new(17u32);
            let mut payload = Some(payload);
            let mut trace = Trace::new(0);
            let result = observing(&mut trace, || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    old.try_extend_in(
                        &resources,
                        2,
                        |index| {
                            if index == 1 {
                                if unwind {
                                    std::panic::resume_unwind(payload.take().unwrap());
                                }
                                return Err(Error::native(-903));
                            }
                            Ok(FaceRegistration {
                                source: source.clone(),
                                index: 17,
                                family: "actual-face",
                            })
                        },
                        &mut [ID::dummy(); 2],
                    )
                }))
            });
            if unwind {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap().unwrap_err(), Error::native(-903));
            }
            assert_eq!(state.lock().unwrap().live, 0);
            assert_eq!(old.len(), 2);
            assert_eq!(old.face(first).unwrap().index, 2);
            assert_orders(&trace, false);
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn wrong_request_and_output_length_fail_before_reservation_and_keep_old_storage() {
        let (old, _, _, source) = base();
        let (first, first_state) = setup();
        let paid = build(&old, &first, &source).unwrap();
        let before = first_state.lock().unwrap().live;
        let (second, second_state) = setup();
        assert_eq!(
            build(&paid, &second, &source).unwrap_err().kind,
            FailureKind::Invariant
        );
        assert_eq!(second_state.lock().unwrap().reserves, 0);
        assert_eq!(first_state.lock().unwrap().live, before);
        let (third, third_state) = setup();
        assert!(old
            .try_extend_in(&third, 1, |_| panic!("must not call factory"), &mut [])
            .is_err());
        assert_eq!(third_state.lock().unwrap().reserves, 0);
        drop(paid);
        assert_eq!(first_state.lock().unwrap().live, 0);
        first.validate_retired().unwrap();
        second.validate_retired().unwrap();
        third.validate_retired().unwrap();
    }

    #[test]
    fn multiple_paid_snapshots_overlap_and_payload_fees_follow_last_strong_owner() {
        let (old, first, _, source) = base();
        let (resources, state) = setup();
        let version_one = Arc::new(build(&old, &resources, &source).unwrap());
        let first_live = state.lock().unwrap().live;
        let alias = version_one.clone();
        let weak = Arc::downgrade(&version_one);
        let version_two = build(&version_one, &resources, &source).unwrap();
        let overlap = state.lock().unwrap().live;
        assert!(overlap > first_live);
        assert_eq!(version_one.len(), 4);
        assert_eq!(version_two.len(), 6);
        compare_face(
            version_one.face(first).unwrap(),
            version_two.face(first).unwrap(),
        );
        drop(version_one);
        assert_eq!(state.lock().unwrap().live, overlap);
        drop(version_two);
        assert_eq!(state.lock().unwrap().live, first_live);
        drop(alias);
        assert!(weak.upgrade().is_none());
        assert_eq!(state.lock().unwrap().live, 0);
        // This checks payload only. The actual new Database Arc allocation
        // belongs to engine RetainedPool's external final-Weak tests.
        drop(weak);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn cleanup_release_failure_preserves_original_native_first_error() {
        let (old, _, _, source) = base();
        let (resources, state) = setup();
        {
            let mut state = state.lock().unwrap();
            state.fail_admission = 3;
            state.fail_release = true;
        }
        assert_eq!(
            build(&old, &resources, &source).unwrap_err(),
            Error::native(-778)
        );
        assert_eq!(resources.failure().unwrap(), Error::native(-778));
        assert!(resources.statistics().release_failures > 0);
        assert!(state.lock().unwrap().live > 0);
        assert!(resources.validate_retired().is_err());
    }
}
