// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual OwnedJob -> E multi-run shaping. Fault only at a real natural refund.

use super::*;
use crate::shape::font_phase::{self, Stage};
use std::cell::RefCell;
use std::sync::atomic::AtomicBool;

const FAULT: Error = Error::native(-1907);
#[derive(Default)]
struct TailHost {
    next: AtomicU32,
    live: Mutex<BTreeMap<u32, (usize, bool)>>,
    arm: AtomicBool,
    hit: AtomicBool,
    failed: Mutex<Option<(u32, usize)>>,
}
impl Host for TailHost {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let slot = self.next.fetch_add(1, Ordering::Relaxed);
        self.live.lock().unwrap().insert(slot, (bytes, false));
        event(3, slot as usize, bytes);
        Ticket::new(0xF07A, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut live = self.live.lock().unwrap();
        let entry = live.get_mut(&ticket.slot()).unwrap();
        assert_eq!(entry.0, bytes);
        assert!(!entry.1);
        entry.1 = true;
        event(4, ticket.slot() as usize, bytes);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut live = self.live.lock().unwrap();
        let entry = *live.get(&ticket.slot()).unwrap();
        event(5, ticket.slot() as usize, 0);
        if self.arm.swap(false, Ordering::Relaxed) {
            assert!(!self.hit.swap(true, Ordering::Relaxed));
            *self.failed.lock().unwrap() = Some((ticket.slot(), entry.0));
            return Err(FAULT);
        }
        live.remove(&ticket.slot());
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}
struct Observation {
    host: Arc<TailHost>,
    fault: bool,
    entered: usize,
    completed: usize,
}
thread_local! { static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) }; }
fn hook(stage: Stage, _: &ResourcesRef) {
    OBSERVATION.with_borrow_mut(|slot| {
        let state = slot.as_mut().unwrap();
        match stage {
            Stage::EnterRun => state.entered += 1,
            Stage::BeforeOwnersDrop => {
                state.completed += 1;
                if state.fault && state.completed == 1 {
                    state.host.arm.store(true, Ordering::Relaxed);
                }
            }
        }
    });
}
struct Installed;
impl Drop for Installed {
    fn drop(&mut self) {
        font_phase::install(None);
        OBSERVATION.with_borrow_mut(|v| {
            v.take();
        });
    }
}

#[test]
fn first_run_natural_owner_refund_failure_stops_the_next_actual_font_run() {
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='250' height='80'><text x='4' y='30' font-family='fixture'>A<tspan direction='rtl' unicode-bidi='bidi-override'>BC</tspan>D</text></svg>";
    for fault in [false, true] {
        let callbacks = install_font();
        let host = Arc::new(TailHost::default());
        let mut job =
            OwnedJob::new(identity(), callbacks, host.clone(), Limits::default()).unwrap();
        OBSERVATION.with_borrow_mut(|v| {
            assert!(
                v.replace(Observation {
                    host: host.clone(),
                    fault,
                    entered: 0,
                    completed: 0
                })
                .is_none()
            );
        });
        assert!(font_phase::install(Some(hook)).is_none());
        let installed = Installed;
        start_events();
        let result = job.prepare(source);
        let events = stop_events();
        let observation = OBSERVATION.with_borrow_mut(|v| v.take().unwrap());
        font_phase::install(None);
        if fault {
            assert_eq!(result, Err(JobError::Failure(FAULT)));
            assert_eq!((observation.entered, observation.completed), (1, 1));
            assert!(host.hit.load(Ordering::Relaxed));
            let (slot, bytes) = host.failed.lock().unwrap().unwrap();
            assert!(bytes > 0);
            let admitted = events
                .iter()
                .position(|e| e.tag == 4 && e.identity == slot as usize && e.bytes == bytes)
                .unwrap();
            let reserved = events
                .iter()
                .position(|e| e.tag == 3 && e.identity == slot as usize && e.bytes == bytes)
                .unwrap();
            let allocated = events[reserved + 1..admitted]
                .iter()
                .find(|e| e.tag == 1 && e.bytes == bytes)
                .unwrap();
            let release = events
                .iter()
                .position(|e| e.tag == 5 && e.identity == slot as usize)
                .unwrap();
            assert!(admitted < release);
            // The original shared System observer witnesses physical free before
            // the failing ticket refund. This is not a pre-recorded error slot.
            assert!(
                events[admitted + 1..release]
                    .iter()
                    .any(|e| e.tag == 2 && e.identity == allocated.identity && e.bytes == bytes)
            );
            let receipt = job.finish_on_worker().unwrap();
            assert_eq!(receipt.first_fault, Some(FAULT));
            // The existing receipt sums Context and Resources observations of
            // this one failed callback. The canonical ticket remains unique.
            assert_eq!(receipt.failed_release_count, 2);
            assert_eq!(receipt.cleanup_status, CleanupStatus::Failed);
            assert_eq!(job.state(), JobState::RetirementFailed);
            assert_eq!(host.live.lock().unwrap().len(), 1);
            assert_eq!(
                *host.live.lock().unwrap().get(&slot).unwrap(),
                (bytes, true)
            );
            println!(
                "E_FONT_TAIL entered=1 completed=1 failed_ticket={slot} failed_bytes={bytes} refund_after_physical_free=true next_run=false"
            );
        } else {
            assert!(result.is_ok(), "{result:?}");
            assert!(
                observation.entered >= 2,
                "the positive input must really create multiple font runs"
            );
            assert_eq!(observation.entered, observation.completed);
            assert!(!host.hit.load(Ordering::Relaxed));
            assert_eq!(
                job.finish_on_worker().unwrap().cleanup_status,
                CleanupStatus::Succeeded
            );
            assert!(host.live.lock().unwrap().is_empty());
            println!("E_FONT_TAIL positive_runs={}", observation.entered);
        }
        FONT.with_borrow_mut(|v| {
            v.take();
        });
        drop(installed);
    }
}
