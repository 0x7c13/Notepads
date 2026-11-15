// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Candidate child module of job_tests. Reuses its sole System observer.

use super::*;
use crate::draft::bidi_phase::{self, Kind};
use markdown_svg_budget::ResourcesRef;
use std::cell::RefCell;

const RELEASE_FAILURE: i32 = -7341;
const NO_TARGET: u32 = u32::MAX;

#[derive(Clone, Copy)]
struct Slot {
    bytes: usize,
    committed: bool,
}

struct ReleaseHost {
    next: AtomicU32,
    slots: Mutex<[Option<Slot>; 512]>,
    target: AtomicU32,
    failed_ticket: Mutex<Option<Ticket>>,
    failed_calls: AtomicU32,
}

impl ReleaseHost {
    fn new() -> Self {
        Self {
            next: AtomicU32::new(0),
            slots: Mutex::new([None; 512]),
            target: AtomicU32::new(NO_TARGET),
            failed_ticket: Mutex::new(None),
            failed_calls: AtomicU32::new(0),
        }
    }
}

impl Host for ReleaseHost {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let slot = self.next.fetch_add(1, Ordering::Relaxed);
        assert!((slot as usize) < 512, "fixed release-test ledger exhausted");
        let mut slots = self.slots.lock().unwrap();
        assert!(slots[slot as usize].is_none());
        slots[slot as usize] = Some(Slot {
            bytes,
            committed: false,
        });
        event(3, slot as usize, bytes);
        Ticket::new(0xB1D2, slot, 1)
    }

    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut slots = self.slots.lock().unwrap();
        let slot = slots[ticket.slot() as usize].as_mut().unwrap();
        assert_eq!(slot.bytes, bytes);
        assert!(!slot.committed);
        slot.committed = true;
        event(4, ticket.slot() as usize, bytes);
        Ok(())
    }

    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        assert_eq!(ticket.account_nonce(), 0xB1D2);
        assert_eq!(ticket.generation(), 1);
        let slot = ticket.slot();
        if slot == self.target.load(Ordering::Relaxed) {
            assert_eq!(self.failed_calls.fetch_add(1, Ordering::Relaxed), 0);
            let record = self.slots.lock().unwrap()[slot as usize].unwrap();
            assert!(record.committed);
            // Distinguish this real failed attempt from successful refunds.
            event(7, slot as usize, record.bytes);
            let mut failed = self.failed_ticket.lock().unwrap();
            assert!(failed.is_none());
            *failed = Some(ticket);
            return Err(Error::native(RELEASE_FAILURE));
        }
        assert!(self.slots.lock().unwrap()[slot as usize].take().is_some());
        event(5, slot as usize, 0);
        Ok(())
    }

    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        // Deliberately continue Ok after the injected release fault. Only the
        // production shared-failure check may stop the following algorithm.
        event(6, work as usize, 0);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct Binding {
    ticket: usize,
    pointer: usize,
    bytes: usize,
    allocated_at: usize,
    live: bool,
}

struct Observation {
    host: Arc<ReleaseHost>,
    request: Option<ResourcesRef>,
    entered: usize,
    exited: usize,
    completed: bool,
    length: usize,
    prefix_end: usize,
    live_outputs: usize,
    target: Option<Binding>,
    native_plan_calls: usize,
    crossed_reorder_consumer: usize,
}

thread_local! {
    static RELEASE_OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) };
}

fn enter(request: &ResourcesRef) {
    RELEASE_OBSERVATION.with_borrow_mut(|slot| {
        let observed = slot.as_mut().unwrap();
        observed.entered += 1;
        assert_eq!(observed.entered, 1, "one fixed production bidi stage");
        observed.request = Some(request.clone());
    });
    start_events();
}

fn exit(completed: bool, length: usize) {
    // Match actual reserve -> System allocation -> commit and prior release
    // events using only stack storage. Observer bookkeeping allocates nothing.
    let (selected, live_outputs, prefix_end) = EVENTS.with_borrow(|events| {
        let mut bindings: [Option<Binding>; 64] = [None; 64];
        let mut count = 0;
        let mut pending: Option<Event> = None;
        for (index, observed) in events.entries[..events.length].iter().enumerate() {
            match observed.tag {
                3 => {
                    assert!(pending.is_none());
                    pending = Some(*observed);
                }
                1 => {
                    let reserved = pending.take().expect("bidi payload was prepaid");
                    assert_eq!(reserved.bytes, observed.bytes);
                    assert!(count < bindings.len());
                    bindings[count] = Some(Binding {
                        ticket: reserved.identity,
                        pointer: observed.identity,
                        bytes: observed.bytes,
                        allocated_at: index,
                        live: true,
                    });
                    count += 1;
                }
                4 => assert!(
                    bindings[..count]
                        .iter()
                        .flatten()
                        .any(|b| b.ticket == observed.identity)
                ),
                5 => {
                    let binding = bindings[..count]
                        .iter_mut()
                        .flatten()
                        .find(|binding| binding.ticket == observed.identity)
                        .expect("refund belongs to this observed bidi stage");
                    binding.live = false;
                }
                _ => {}
            }
        }
        assert!(pending.is_none());
        let mut live_outputs = 0;
        let mut selected = None;
        for binding in bindings[..count]
            .iter()
            .flatten()
            .filter(|binding| binding.live)
        {
            live_outputs += 1;
            if selected.is_none() {
                selected = Some(*binding);
            }
        }
        (selected, live_outputs, events.length)
    });
    RELEASE_OBSERVATION.with_borrow_mut(|slot| {
        let observed = slot.as_mut().unwrap();
        observed.exited += 1;
        observed.completed = completed;
        observed.length = length;
        observed.prefix_end = prefix_end;
        observed.live_outputs = live_outputs;
        observed.target = selected;
        let selected = selected.expect("completed nonempty bidi result owns live output");
        assert_eq!(
            observed
                .host
                .target
                .swap(selected.ticket as u32, Ordering::Relaxed),
            NO_TARGET
        );
    });
    // Continue observing the actual selected payload's later free and callback.
}

fn shape_retired(request: &ResourcesRef) {
    RELEASE_OBSERVATION.with_borrow_mut(|slot| {
        let observed = slot.as_mut().unwrap();
        assert!(request.is_same_request(observed.request.as_ref().unwrap()));
        observed.crossed_reorder_consumer += 1;
        // Observation only: never reject or mask the negative producer path.
    });
}

unsafe extern "C" fn counting_no_plan(_: u64, _: *const abi::Draft, _: *mut abi::Plan) -> i32 {
    RELEASE_OBSERVATION.with_borrow_mut(|slot| {
        slot.as_mut().unwrap().native_plan_calls += 1;
    });
    -101
}

struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        bidi_phase::install(None);
        bidi_phase::install_shape_retired(None);
        EVENTS.with_borrow_mut(|events| events.enabled = false);
        RELEASE_OBSERVATION.with_borrow_mut(|slot| {
            slot.take();
        });
        FONT.with_borrow_mut(|slot| {
            slot.take();
        });
    }
}

fn exercise(
    kind: Kind,
) -> (
    Observation,
    Vec<Event>,
    Retirement,
    Result<NaturalSize, JobError>,
) {
    let host = Arc::new(ReleaseHost::new());
    let fonts = if kind == Kind::Reorder {
        install_font()
    } else {
        let mut callbacks = callbacks();
        callbacks.plan = Some(counting_no_plan);
        callbacks
    };
    let mut job = OwnedJob::new(identity(), fonts, host.clone(), Limits::default()).unwrap();
    RELEASE_OBSERVATION.with_borrow_mut(|slot| {
        assert!(slot.is_none());
        *slot = Some(Observation {
            host: host.clone(),
            request: None,
            entered: 0,
            exited: 0,
            completed: false,
            length: 0,
            prefix_end: 0,
            live_outputs: 0,
            target: None,
            native_plan_calls: 0,
            crossed_reorder_consumer: 0,
        });
    });
    assert!(bidi_phase::install(Some(bidi_phase::Hooks { kind, enter, exit })).is_none());
    assert!(bidi_phase::install_shape_retired(Some(shape_retired)).is_none());
    let installed = Installed;
    let source = if kind == Kind::Draft {
        b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>A (&#x5D0; 12) &#x2067;B&#x2069; &#x202B;CD&#x202C;</text></svg>".as_slice()
    } else {
        b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>office A&amp;B</text></svg>".as_slice()
    };
    let outcome = job.prepare(source);
    let events = stop_events();
    let mut observed = RELEASE_OBSERVATION.with_borrow_mut(|slot| slot.take().unwrap());
    assert_eq!(
        observed.request.as_ref().unwrap().failure(),
        Some(Error::native(RELEASE_FAILURE))
    );
    drop(observed.request.take());
    drop(installed);

    assert_eq!(job.state(), JobState::Failed);
    assert_eq!(
        job.stats().unwrap().first_fault,
        Some(Error::native(RELEASE_FAILURE))
    );
    assert_eq!(job.set_target(target(64.0)), Err(JobError::InvalidState));
    let mut pixels = [0xA5; 4];
    assert_eq!(
        job.run_into(identity(), &mut pixels, 4),
        Err(JobError::InvalidState)
    );
    assert_eq!(pixels, [0xA5; 4]);
    let retirement = job.finish_on_worker().unwrap();
    assert_eq!(retirement.first_fault, Some(Error::native(RELEASE_FAILURE)));
    assert_eq!(retirement.cleanup_status, CleanupStatus::Failed);
    assert_eq!(retirement.remaining_resource_owners, 1);
    // One failed callback is counted by Context and by the Resources ledger.
    assert_eq!(retirement.failed_release_count, 2);
    assert_eq!(job.state(), JobState::RetirementFailed);
    assert!(!job.can_destroy_shell());
    assert_eq!(host.failed_calls.load(Ordering::Relaxed), 1);
    let target = observed.target.unwrap();
    let failed = host.failed_ticket.lock().unwrap();
    assert_eq!(failed.as_ref().unwrap().slot() as usize, target.ticket);
    drop(failed);
    let slots = host.slots.lock().unwrap();
    assert_eq!(slots.iter().flatten().count(), 1);
    assert_eq!(slots[target.ticket].unwrap().bytes, target.bytes);
    drop(slots);
    eprintln!(
        "owned_bidi_release kind={kind:?} completed={} length={} live_outputs_at_complete={} target={target:?} native_plan_calls={} crossed_reorder_consumer={} outcome={outcome:?} retirement={retirement:?}",
        observed.completed,
        observed.length,
        observed.live_outputs,
        observed.native_plan_calls,
        observed.crossed_reorder_consumer
    );
    eprintln!("owned_bidi_release_events kind={kind:?} {events:?}");
    // RetirementFailed deliberately quarantines canonical work; no retry,
    // refund or false successful finish is introduced by this test.
    (observed, events, retirement, outcome)
}

fn verify_selected_free_before_failed_release(observed: &Observation, events: &[Event]) {
    assert_eq!(observed.entered, 1);
    assert_eq!(observed.exited, 1);
    assert!(observed.completed && observed.length > 0);
    let selected = observed.target.unwrap();
    let freed = events
        .iter()
        .enumerate()
        .skip(selected.allocated_at + 1)
        .find(|(_, event)| event.tag == 2 && event.identity == selected.pointer)
        .map(|(index, _)| index)
        .expect("real System free for selected owner");
    let failed_release = events
        .iter()
        .enumerate()
        .skip(observed.prefix_end)
        .find(|(_, event)| event.tag == 7 && event.identity == selected.ticket)
        .map(|(index, _)| index)
        .expect("selected canonical failed-release ticket");
    assert!(selected.allocated_at < freed && freed < failed_release);
    assert!(
        !events
            .iter()
            .any(|event| event.tag == 5 && event.identity == selected.ticket)
    );
}

#[test]
fn owned_draft_bidi_release_failure_stops_before_native_plan() {
    let (observed, events, _, outcome) = exercise(Kind::Draft);
    verify_selected_free_before_failed_release(&observed, &events);
    assert_eq!(
        observed.live_outputs, 3,
        "classes, levels, paragraphs remain live at complete"
    );
    assert_eq!(
        outcome,
        Err(JobError::Failure(Error::native(RELEASE_FAILURE)))
    );
    assert_eq!(
        observed.native_plan_calls, 0,
        "draft must observe paid bidi destructor's first fault before native plan"
    );
}

#[test]
fn owned_shape_bidi_release_failure_stops_before_reorder_consumer_boundary() {
    let (observed, events, _, outcome) = exercise(Kind::Reorder);
    verify_selected_free_before_failed_release(&observed, &events);
    assert_eq!(
        observed.live_outputs, 1,
        "one paid permutation remains live at complete"
    );
    assert_eq!(
        outcome,
        Err(JobError::Failure(Error::native(RELEASE_FAILURE)))
    );
    assert_eq!(
        observed.crossed_reorder_consumer, 0,
        "first fault must stop before leaving the paragraph-order consumption boundary"
    );
}
