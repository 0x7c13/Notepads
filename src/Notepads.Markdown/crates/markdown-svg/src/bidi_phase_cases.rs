// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual DraftOwner/OwnedBidiOrder probes. No second allocator or bidi algorithm.
use super::*;
use crate::draft::bidi_phase::{self, Kind};
use std::cell::RefCell;

struct BidiHost {
    next: AtomicU32,
    slots: Mutex<[Option<(usize, bool)>; 2048]>,
}
impl BidiHost {
    fn new() -> Self {
        Self {
            next: AtomicU32::new(0),
            slots: Mutex::new([None; 2048]),
        }
    }
}
impl Host for BidiHost {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let slot = self.next.fetch_add(1, Ordering::Relaxed);
        assert!((slot as usize) < 2048, "fixed test-host ledger exhausted");
        let mut slots = self.slots.lock().unwrap();
        assert!(slots[slot as usize].is_none());
        slots[slot as usize] = Some((bytes, false));
        event(3, slot as usize, bytes);
        Ticket::new(0xB1D1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        event(4, ticket.slot() as usize, bytes);
        let mut slots = self.slots.lock().unwrap();
        let entry = slots[ticket.slot() as usize].as_mut().unwrap();
        assert_eq!(entry.0, bytes);
        assert!(!entry.1);
        entry.1 = true;
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        assert_eq!(ticket.account_nonce(), 0xB1D1);
        let entry = self.slots.lock().unwrap()[ticket.slot() as usize].take();
        assert!(entry.is_some());
        event(5, ticket.slot() as usize, 0);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        event(6, work as usize, 0);
        Ok(())
    }
}

struct Observation {
    cancel: Option<CancelHandle>,
    entered: usize,
    exited: usize,
    completed: bool,
    length: usize,
    prefix_end: usize,
    entry_scratch: u64,
}
thread_local! {
    static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) };
}
fn enter(resources: &ResourcesRef) {
    OBSERVATION.with_borrow_mut(|slot| {
        let observation = slot.as_mut().unwrap();
        observation.entered += 1;
        assert_eq!(
            observation.entered, 1,
            "one target stage in the fixed source"
        );
        observation.entry_scratch = resources.statistics().scratch_bytes;
        if let Some(cancel) = &observation.cancel {
            // Actual cancellation is pending BEFORE the production bidi call.
            cancel.cancel();
        }
    });
    start_events();
}
fn exit(completed: bool, length: usize) {
    let prefix_end = EVENTS.with_borrow(|events| events.length);
    OBSERVATION.with_borrow_mut(|slot| {
        let observation = slot.as_mut().unwrap();
        observation.exited += 1;
        observation.completed = completed;
        observation.length = length;
        observation.prefix_end = prefix_end;
    });
    // Keep observing actual target frees after the result leaves the bidi call.
    // Later allocations are not classified as bidi by the prefix verifier.
}
struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        bidi_phase::install(None);
        EVENTS.with_borrow_mut(|events| events.enabled = false);
        OBSERVATION.with_borrow_mut(|slot| {
            slot.take();
        });
    }
}

fn exercise(kind: Kind, cancel_before: bool) -> (Observation, Vec<Event>) {
    let host = Arc::new(BidiHost::new());
    let fonts = if kind == Kind::Reorder {
        install_font()
    } else {
        callbacks()
    };
    let mut job = OwnedJob::new(identity(), fonts, host.clone(), Limits::default()).unwrap();
    OBSERVATION.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(Observation {
            cancel: cancel_before.then(|| job.cancellation()),
            entered: 0,
            exited: 0,
            completed: false,
            length: 0,
            prefix_end: 0,
            entry_scratch: 0,
        });
    });
    assert!(bidi_phase::install(Some(bidi_phase::Hooks { kind, enter, exit })).is_none());
    let installed = Installed;
    let source = if kind == Kind::Draft {
        b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>A (&#x5D0; 12) &#x2067;B&#x2069; &#x202B;CD&#x202C;</text></svg>".as_slice()
    } else {
        b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>office A&amp;B</text></svg>".as_slice()
    };
    let outcome = job.prepare(source);
    let events = stop_events();
    let observation = OBSERVATION.with_borrow_mut(|slot| slot.take().unwrap());
    bidi_phase::install(None);
    let state_before_retirement = job.state();
    let retirement = job.finish_on_worker().unwrap();
    assert_eq!(retirement.cleanup_status, CleanupStatus::Succeeded);
    assert_eq!(retirement.remaining_resource_owners, 0);
    assert_eq!(retirement.failed_release_count, 0);
    assert!(host.slots.lock().unwrap().iter().all(Option::is_none));
    if kind == Kind::Reorder {
        FONT.with_borrow_mut(|slot| {
            slot.take();
        });
    }
    drop(installed);
    assert_eq!(observation.entered, 1);
    assert_eq!(observation.exited, 1);
    let prefix = &events[..observation.prefix_end];
    let allocations = prefix.iter().filter(|event| event.tag == 1).count();
    let reserves = prefix.iter().filter(|event| event.tag == 3).count();
    let work_checks = prefix.iter().filter(|event| event.tag == 6).count();
    eprintln!(
        "bidi_actual_stage kind={kind:?} cancel_before={cancel_before} completed={} length={} actual_allocations={allocations} reserves={reserves} work_checks={work_checks} prefix_events={} total_events={} entry_scratch={} outcome={outcome:?} state_before_retirement={state_before_retirement:?} first_fault={:?} retired_owners=0 release_failures=0",
        observation.completed,
        observation.length,
        observation.prefix_end,
        events.len(),
        observation.entry_scratch,
        retirement.first_fault
    );
    eprintln!("bidi_actual_stage_events kind={kind:?} cancel_before={cancel_before} {events:?}");
    (observation, events)
}

#[test]
fn bidi_stage_draft_all_actual_payloads_are_prepaid_and_freed_before_refund() {
    let (observed, events) = exercise(Kind::Draft, false);
    assert!(observed.completed && observed.length > 0);
    let prefix = &events[..observed.prefix_end];
    assert!(
        prefix.iter().any(|event| event.tag == 1),
        "nonempty real bidi"
    );
    let mut pending: Option<Event> = None;
    let mut bindings: Vec<(usize, usize, usize)> = Vec::new();
    for (index, observed) in prefix.iter().enumerate() {
        match observed.tag {
            3 => {
                assert!(pending.is_none(), "no unresolved payload reservation");
                pending = Some(*observed);
            }
            1 => {
                let reserved = pending
                    .take()
                    .expect("actual bidi allocation needs prior payment");
                assert_eq!(observed.bytes, reserved.bytes);
                bindings.push((reserved.identity, observed.identity, index));
            }
            4 => {
                assert!(
                    bindings
                        .iter()
                        .any(|binding| binding.0 == observed.identity)
                );
            }
            _ => {}
        }
    }
    assert!(pending.is_none());
    for (ticket, pointer, allocated_at) in bindings {
        let freed = events
            .iter()
            .enumerate()
            .skip(allocated_at + 1)
            .find(|(_, observed)| observed.tag == 2 && observed.identity == pointer)
            .map(|(index, _)| index)
            .expect("actual System deallocation observed");
        let refunded = events
            .iter()
            .enumerate()
            .skip(allocated_at + 1)
            .find(|(_, observed)| observed.tag == 5 && observed.identity == ticket)
            .map(|(index, _)| index)
            .expect("actual target ticket release observed");
        assert!(
            freed < refunded,
            "payload must physically free before refund"
        );
    }
    assert!(
        prefix.iter().any(|event| event.tag == 6),
        "internal work must be checked"
    );
}

#[test]
fn bidi_stage_draft_pending_cancel_never_constructs_a_result() {
    let (observed, events) = exercise(Kind::Draft, true);
    assert!(
        !observed.completed,
        "actual draft bidi ignored pending cancellation"
    );
    assert!(
        !events[..observed.prefix_end]
            .iter()
            .any(|event| event.tag == 1)
    );
}

#[test]
fn bidi_stage_reorder_internal_work_is_checked_not_just_payload_prepaid() {
    let (observed, events) = exercise(Kind::Reorder, false);
    assert!(observed.completed && observed.length > 0);
    assert!(
        events[..observed.prefix_end]
            .iter()
            .any(|event| event.tag == 6),
        "the old exact payload lease does not pay/check reorder visits"
    );
}

#[test]
fn bidi_stage_reorder_pending_cancel_never_constructs_a_result() {
    let (observed, events) = exercise(Kind::Reorder, true);
    assert!(
        !observed.completed,
        "actual permutation ignored pending cancellation"
    );
    assert!(
        !events[..observed.prefix_end]
            .iter()
            .any(|event| event.tag == 1)
    );
}
