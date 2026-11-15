// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pre-execution, handwritten decoded-ASCII matcher assertions. Not a decoder,
//! parser, email commit, source-atom, Core Q3/Q4 model, or performance test.
//! Q3/Q4 below supply their independently known decoded values, not raw HTML
//! entities. Private numeric node tags are fixture identities, not proof that
//! grammar accepted those nodes or issued a source capability.
//!
//! This small Host tracks unique permit IDs/capacities/Drop and logical fault
//! injection. It has no allocator-address observation and cannot dynamically
//! prove physical frees, real OS allocation failure or full 32/16 fee coverage.
//! Test fixture/observer Strings and BTreeMap allocations are outside producer
//! accounting. No parser, decoder, native event tape, source token or AST exists.

use super::super::with_run;
use super::*;
use crate::native::{AcceptedFact, NativeNodeId};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Budget,
    Cancel,
    Allocation,
    Invalid,
}

#[derive(Default)]
struct Book {
    issued: u64,
    dropped: u64,
    live: BTreeMap<u64, u64>,
}
struct Permit {
    id: u64,
    book: Rc<RefCell<Book>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut book = self.book.borrow_mut();
        assert!(book.live.remove(&self.id).is_some());
        book.dropped = book.dropped.checked_add(1).unwrap();
    }
}
#[derive(Default)]
struct Meter {
    book: Rc<RefCell<Book>>,
    first: Option<Fault>,
    records: u64,
    scan: u64,
    copy: u64,
    events: u64,
    reserve_calls: u64,
    actual_calls: u64,
    record_limit: Option<u64>,
    cancel: bool,
    cancel_record: Option<u64>,
    cancel_event: Option<u64>,
    fail_reserve: Option<u64>,
    fail_actual: Option<u64>,
}
impl Meter {
    fn fail(&mut self, value: Fault) -> Fault {
        *self.first.get_or_insert(value)
    }
    fn ready(&mut self) -> Result<(), Fault> {
        if let Some(first) = self.first {
            return Err(first);
        }
        if self.cancel {
            return Err(self.fail(Fault::Cancel));
        }
        Ok(())
    }
}
impl Host for Meter {
    type Error = Fault;
    type Lease = Permit;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Fault> {
        self.ready()?;
        match delta {
            WorkDelta::Scan(n) => self.scan = self.scan.checked_add(n).unwrap(),
            WorkDelta::Copy(n) => self.copy = self.copy.checked_add(n).unwrap(),
            WorkDelta::Events(n) => {
                let next = self.events.checked_add(n).unwrap();
                if self.cancel_event.is_some_and(|at| next >= at) {
                    return Err(self.fail(Fault::Cancel));
                }
                self.events = next;
            }
        }
        Ok(())
    }
    fn admit(&mut self, request: NativeAdmission) -> Result<(), Fault> {
        self.ready()?;
        let NativeAdmission::Record(n) = request else {
            panic!("numeric matcher must not admit nodes or decoded strings");
        };
        let next = self.records.checked_add(n).unwrap();
        if self.cancel_record.is_some_and(|at| next >= at) {
            return Err(self.fail(Fault::Cancel));
        }
        if self.record_limit.is_some_and(|limit| next > limit) {
            return Err(self.fail(Fault::Budget));
        }
        self.records = next;
        Ok(())
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Fault> {
        self.ready()?;
        self.reserve_calls = self.reserve_calls.checked_add(1).unwrap();
        if self.fail_reserve == Some(self.reserve_calls) {
            return Err(self.fail(Fault::Allocation));
        }
        let mut book = self.book.borrow_mut();
        book.issued = book.issued.checked_add(1).unwrap();
        let id = book.issued;
        assert!(book.live.insert(id, bytes).is_none());
        Ok(Permit {
            id,
            book: self.book.clone(),
        })
    }
    fn admit_actual(&mut self, lease: &mut Permit, bytes: u64) -> Result<(), Fault> {
        self.ready()?;
        self.actual_calls = self.actual_calls.checked_add(1).unwrap();
        *self.book.borrow_mut().live.get_mut(&lease.id).unwrap() = bytes;
        if self.fail_actual == Some(self.actual_calls) {
            return Err(self.fail(Fault::Allocation));
        }
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Fault> {
        panic!("numeric matcher must not publish native grammar facts")
    }
    fn reject(&mut self, _: NativeViolation) -> Fault {
        self.fail(Fault::Invalid)
    }
}

fn tag(value: u32) -> NativeNodeId {
    NativeNodeId::from_raw(value).unwrap()
}
fn final_zero(book: &Rc<RefCell<Book>>) {
    let book = book.borrow();
    assert!(book.live.is_empty());
    assert_eq!(book.dropped, book.issued);
}
fn check(decoded: &str, expected: &[(usize, usize, usize)]) {
    let mut meter = Meter::default();
    let book = meter.book.clone();
    with_run(tag(1), decoded.len(), |run| {
        let mut ctx = Context::new(&mut meter, decoded.len());
        let mut selection = select(decoded, run, &mut ctx).unwrap();
        assert_eq!(selection.selected.len(), expected.len());
        if expected.is_empty() {
            assert_eq!(ctx.host.reserve_calls, 0);
        } else {
            assert!(!book.borrow().live.is_empty());
            assert!(
                ctx.host.copy >= (expected.len() * std::mem::size_of::<EmailMatch<'_>>()) as u64
            );
        }
        for &(start, at, end) in expected {
            let item = selection
                .next(tag(1), decoded.len(), &mut ctx)
                .unwrap()
                .unwrap();
            assert_eq!((item.start(), item.at(), item.end()), (start, at, end));
            assert!(decoded.is_char_boundary(start));
            assert!(decoded.is_char_boundary(end));
        }
        assert!(
            selection
                .next(tag(1), decoded.len(), &mut ctx)
                .unwrap()
                .is_none()
        );
        assert!(
            selection
                .next(tag(1), decoded.len(), &mut ctx)
                .unwrap()
                .is_none()
        );
        drop(selection);
        // The still-live Context has no matcher scratch lease to conceal here.
        final_zero(&book);
    });
    assert_eq!(meter.records, expected.len() as u64);
    assert_eq!(meter.first, None);
    final_zero(&book);
}

#[test]
fn decoded_q3_q4_and_escape_value_have_exact_ascii_intervals() {
    // Q3 a&#64;b.com -> a@b.com; Q4 &fjlig;@a.com -> fj@a.com.
    // These inputs do NOT exercise the corresponding decoder or whole Transform.
    check("a@b.com", &[(0, 1, 7)]);
    check("fj@a.com", &[(0, 2, 8)]);
    check("a_b@c.com", &[(0, 3, 9)]);
    check("A+B_c-D.E@F_g.H2", &[(0, 9, 16)]);
}

#[test]
fn whole_domain_validation_excludes_only_trailing_dots() {
    for (decoded, expected) in [
        ("a@b.com...", (0, 1, 7)),
        ("a@b.3", (0, 1, 5)),
        ("a@-b.c", (0, 1, 6)),
        ("a@b_.c", (0, 1, 6)),
        ("a@b.c-d", (0, 1, 7)),
    ] {
        check(decoded, &[expected]);
    }
    for decoded in [
        "",
        "@a.b",
        "a@b",
        "a@.b.c",
        "a@b..c",
        "a@b...c...",
        "a@b.c-",
        "a@b.c_",
        "a@b.c-...",
        "a@b.c_...",
        "a@...",
        "a@中.c",
        "a@b+c.d",
        "a@@b.com",
    ] {
        check(decoded, &[]);
    }
}

#[test]
fn previous_at_finishes_before_cursor_clipping_and_plus_keeps_local_state() {
    check("a@b.com@c.com", &[(0, 1, 7)]);
    check("a@b.com+z@d.e", &[(0, 1, 7), (7, 9, 13)]);
    check("a@b.com..@c.com", &[(0, 1, 7), (7, 9, 15)]);
    check("a@b..c@d.e", &[(2, 6, 10)]);
    check("a@b.com x@y.z", &[(0, 1, 7), (8, 9, 13)]);
    check("p+q@a.b+r@s.t", &[(0, 3, 7), (7, 9, 13)]);
}

#[test]
fn unicode_barriers_brackets_and_protocol_literal_do_not_add_a_boundary_gate() {
    check("中a@b.com", &[(3, 4, 10)]);
    check("中a@b.com文c@d.e", &[(3, 4, 10), (13, 14, 18)]);
    check("[a@b.com", &[(1, 2, 8)]);
    check("]a@b.com", &[(1, 2, 8)]);
    check("(a@b.com)", &[(1, 2, 8)]);
    check("mailto:a@b.com", &[(7, 8, 14)]);
    check("xmpp:a@b.com", &[(5, 6, 12)]);
}

#[test]
fn long_failed_tokens_have_one_byte_scan_and_no_candidate_rows_or_allocations() {
    // Finite mechanism controls, not pressure/cost qualification. Construction
    // is test fixture work. Last -/_ and internal empty must reject the token.
    let inputs = [
        format!("a@{}_", "x.".repeat(1024)),
        format!("a@{}..y", "x.".repeat(1024)),
        format!("{}b_", "a@".repeat(1024)),
    ];
    for decoded in &inputs {
        let mut meter = Meter::default();
        let book = meter.book.clone();
        with_run(tag(1), decoded.len(), |run| {
            let mut ctx = Context::new(&mut meter, decoded.len());
            let selection = select(decoded, run, &mut ctx).unwrap();
            assert_eq!(selection.selected.len(), 0);
            // Before reader calls: zero rows means no descriptor reads/writes;
            // each actual D byte was read/classified once and the tail flushed.
            assert_eq!(ctx.host.scan, decoded.len() as u64);
            assert_eq!(ctx.host.copy, 0);
            assert_eq!(ctx.host.records, 0);
            assert_eq!(ctx.host.reserve_calls, 0);
            drop(selection);
            final_zero(&book);
        });
    }
}

#[test]
fn wrong_run_length_and_wrong_reader_identity_reject_without_source_authority() {
    let decoded = "a@b.c";
    for bytes in [decoded.len() - 1, decoded.len() + 1] {
        let mut meter = Meter::default();
        let book = meter.book.clone();
        with_run(tag(1), bytes, |run| {
            let mut ctx = Context::new(&mut meter, decoded.len());
            assert_eq!(select(decoded, run, &mut ctx).err(), Some(Fault::Invalid));
        });
        assert_eq!(meter.scan, 0);
        assert_eq!(meter.records, 0);
        final_zero(&book);
    }
    for (node, bytes) in [(tag(2), decoded.len()), (tag(1), decoded.len() + 1)] {
        let mut meter = Meter::default();
        let book = meter.book.clone();
        with_run(tag(1), decoded.len(), |run| {
            let mut ctx = Context::new(&mut meter, decoded.len());
            let mut selection = select(decoded, run, &mut ctx).unwrap();
            assert_eq!(
                selection.next(node, bytes, &mut ctx).err(),
                Some(Fault::Invalid)
            );
            assert_eq!(
                selection.next(tag(1), decoded.len(), &mut ctx).err(),
                Some(Fault::Invalid)
            );
            drop(selection);
            final_zero(&book);
        });
    }
}

fn two_matches(meter: &mut Meter, expected_fault: Option<Fault>) {
    let decoded = "a@b.c d@e.f";
    let book = meter.book.clone();
    with_run(tag(1), decoded.len(), |run| {
        let mut ctx = Context::new(meter, decoded.len());
        match (select(decoded, run, &mut ctx), expected_fault) {
            (Ok(mut selection), None) => {
                for expected in [(0, 1, 5), (6, 7, 11)] {
                    let row = selection
                        .next(tag(1), decoded.len(), &mut ctx)
                        .unwrap()
                        .unwrap();
                    assert_eq!((row.start(), row.at(), row.end()), expected);
                }
                assert!(
                    selection
                        .next(tag(1), decoded.len(), &mut ctx)
                        .unwrap()
                        .is_none()
                );
                drop(selection);
            }
            (Err(actual), Some(expected)) => assert_eq!(actual, expected),
            _ => panic!("wrong complete-selection result; no partial selection may escape"),
        }
        final_zero(&book);
    });
    assert_eq!(meter.first, expected_fault);
    final_zero(&book);
}

#[test]
fn exact_and_one_short_record_caps_and_first_fault_cancellation_release_rows() {
    let mut exact = Meter {
        record_limit: Some(2),
        ..Meter::default()
    };
    two_matches(&mut exact, None);
    assert_eq!(exact.records, 2);
    let mut short = Meter {
        record_limit: Some(1),
        ..Meter::default()
    };
    two_matches(&mut short, Some(Fault::Budget));
    assert_eq!(short.records, 1);
    assert!(
        short.book.borrow().issued > 0,
        "one real accepted row existed before rejection"
    );
    let mut zero = Meter {
        record_limit: Some(0),
        ..Meter::default()
    };
    two_matches(&mut zero, Some(Fault::Budget));
    assert_eq!(zero.reserve_calls, 0);
    let mut cancelled = Meter {
        cancel_record: Some(2),
        record_limit: Some(1),
        ..Meter::default()
    };
    two_matches(&mut cancelled, Some(Fault::Cancel));
    assert_eq!(cancelled.records, 1);
    let mut early = Meter {
        cancel_event: Some(4),
        ..Meter::default()
    };
    two_matches(&mut early, Some(Fault::Cancel));
    assert_eq!(early.reserve_calls, 0);
    let mut latched = Meter {
        first: Some(Fault::Budget),
        cancel: true,
        ..Meter::default()
    };
    two_matches(&mut latched, Some(Fault::Budget));
    assert_eq!(latched.scan, 0);
}

#[test]
fn directory_and_page_pre_and_post_allocation_failures_release_all_permits() {
    // NativeRows' first accepted row reserves directory then page. Each hook
    // is independently injected; no expected outcome is calibrated from a run.
    // Post-allocation admit_actual failure observes the new lease then drops it.
    for at in [1, 2] {
        let mut before = Meter {
            fail_reserve: Some(at),
            ..Meter::default()
        };
        two_matches(&mut before, Some(Fault::Allocation));
        assert_eq!(before.reserve_calls, at);
        assert_eq!(before.book.borrow().issued, at - 1);
        let mut after = Meter {
            fail_actual: Some(at),
            ..Meter::default()
        };
        two_matches(&mut after, Some(Fault::Allocation));
        assert_eq!(after.actual_calls, at);
        assert_eq!(after.book.borrow().issued, at);
    }
}
