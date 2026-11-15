// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Prewritten B mechanism assertions only. Literals/selection policy come from
// fixed-frame-proof-research-v1/hand-expected.json and approved-g4-contract-v3.
// Owner rows below are explicit lemma inputs, NOT a substitute for native A.
// No parser, AcceptedModel, query, canonical replay, or cost qualification here.

use super::super::super::{AcceptedFact, BufferId, NativeNodeId};
use super::super::with_leaf;
use super::*;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Invalid,
    Budget,
    Cancel,
}
struct Permit {
    bytes: u64,
    live: Rc<Cell<u64>>,
    count: Rc<Cell<usize>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.live
            .set(self.live.get().checked_sub(self.bytes).unwrap());
        self.count.set(self.count.get().checked_sub(1).unwrap());
    }
}
#[derive(Default)]
struct Meter {
    live: Rc<Cell<u64>>,
    permits: Rc<Cell<usize>>,
    records: u64,
    scan: u64,
    copy: u64,
    events: u64,
    reserves: usize,
    first: Option<Fault>,
    fail_record: Option<(u64, Fault)>,
    fail_reserve: Option<usize>,
}
impl Meter {
    fn ready(&self) -> Result<(), Fault> {
        self.first.map_or(Ok(()), Err)
    }
    fn fault(&mut self, fault: Fault) -> Fault {
        *self.first.get_or_insert(fault)
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
            WorkDelta::Events(n) => self.events = self.events.checked_add(n).unwrap(),
        }
        Ok(())
    }
    fn admit(&mut self, request: NativeAdmission) -> Result<(), Fault> {
        self.ready()?;
        let NativeAdmission::Record(n) = request else {
            panic!("unexpected B admission")
        };
        let next = self.records.checked_add(n).unwrap();
        if let Some((at, fault)) = self.fail_record
            && next >= at
        {
            return Err(self.fault(fault));
        }
        self.records = next;
        Ok(())
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Fault> {
        self.ready()?;
        self.reserves += 1;
        if self.fail_reserve == Some(self.reserves) {
            return Err(self.fault(Fault::Budget));
        }
        self.live.set(self.live.get().checked_add(bytes).unwrap());
        self.permits.set(self.permits.get().checked_add(1).unwrap());
        Ok(Permit {
            bytes,
            live: self.live.clone(),
            count: self.permits.clone(),
        })
    }
    fn admit_actual(&mut self, lease: &mut Permit, bytes: u64) -> Result<(), Fault> {
        self.ready()?;
        self.live.set(
            self.live
                .get()
                .checked_sub(lease.bytes)
                .unwrap()
                .checked_add(bytes)
                .unwrap(),
        );
        lease.bytes = bytes;
        Ok(())
    }
    fn fact(&mut self, _: AcceptedFact<'_>) -> Result<(), Fault> {
        panic!("B must not publish grammar facts")
    }
    fn reject(&mut self, _: NativeViolation) -> Fault {
        self.fault(Fault::Invalid)
    }
}

// (kind, start, end, parent, exclusive subtree_end).
type Owner = (OwnerKind, usize, usize, Option<usize>, usize);
fn released<'leaf>(
    leaf: LeafBrand<'leaf>,
    input: &[Owner],
    ctx: &mut Context<'_, Meter>,
) -> ShadowReleased<'leaf, Permit> {
    let mut owners = NativeRows::default();
    for (index, &(kind, start, end, parent, subtree_end)) in input.iter().enumerate() {
        append(
            &mut owners,
            ShadowOwner {
                leaf,
                kind,
                start,
                end,
                parent,
                subtree_end,
                opening_slot: u64::try_from(index).unwrap() + 1,
            },
            ctx,
        )
        .unwrap();
    }
    ShadowReleased { leaf, owners }
}

fn check(raw: &str, owners: &[Owner], expected: &[(SelectedKind, usize, usize, usize)]) {
    let mut meter = Meter::default();
    with_leaf(
        NativeNodeId::from_raw(1).unwrap(),
        BufferId::from_raw(1).unwrap(),
        42,
        raw.len(),
        |leaf| {
            let mut ctx = Context::new(&mut meter, raw.len());
            let shadow = released(leaf, owners, &mut ctx);
            let mut result = select(raw, shadow, &mut ctx).unwrap();
            // Small controls fit one directory/page each. No owner or stack permit
            // may remain: only the result's two independently held leases survive.
            assert_eq!(
                ctx.host.permits.get(),
                if expected.is_empty() { 0 } else { 2 }
            );
            for &(kind, start, end, domain_end) in expected {
                let link = result
                    .next(&mut ctx)
                    .unwrap()
                    .expect("prewritten selected row");
                assert_eq!(
                    (link.kind(), link.start(), link.end(), link.domain_end()),
                    (kind, start, end, domain_end)
                );
                assert!(same_leaf(link.leaf, leaf));
            }
            assert!(result.next(&mut ctx).unwrap().is_none());
            drop(result);
            assert_eq!(ctx.host.live.get(), 0);
            assert_eq!(ctx.host.permits.get(), 0);
        },
    );
    assert!(meter.scan > 0 && meter.events > 0);
    if !expected.is_empty() {
        assert!(meter.copy > 0);
    }
    assert_eq!(meter.first, None);
    assert_eq!(meter.live.get(), 0);
}

#[test]
fn fixed_literals_prefix_domain_utf8_and_tail() {
    use SelectedKind::{Url, Www};
    check("[ www.a.com", &[], &[(Www, 2, 11, 11)]);
    check("HTTPS://a.b", &[], &[(Url, 0, 11, 11)]);
    check("www.汉字.example", &[], &[(Www, 0, 18, 18)]);
    check("www.a_b.c.d", &[], &[(Www, 0, 11, 11)]);
    check("www.a..b_www.c.d...", &[], &[(Www, 9, 16, 16)]);
    check(
        "www.a.com/<(www.a.com/<(z",
        &[],
        &[(Www, 0, 10, 9), (Www, 12, 22, 21)],
    );
    check("www.a.com/&a1;)))?!", &[], &[(Www, 0, 10, 9)]);
    check("www.a.com/&#64;", &[], &[(Www, 0, 15, 9)]);
    check("www.a.com/x'\"", &[], &[(Www, 0, 13, 9)]);
    check("www.a.com/(x))", &[], &[(Www, 0, 13, 9)]);
}

#[test]
fn fixed_negative_domains_and_raw_boundaries() {
    for raw in [
        "[www.a.com",
        "WWW.a.b",
        "xhttp://a.b",
        "&#32;www.a.com",
        "http&#58;//a.com",
        "www.a",
        "http://localhost",
        "www.a..b...",
        "www._a.b",
        "_www.a.b_www.c.d_",
    ] {
        check(raw, &[], &[]);
    }
}

#[test]
fn fixed_summary_failure_keeps_suffix_state() {
    let raw = "www.a..b_www.c.d...";
    let mut meter = Meter::default();
    let mut ctx = Context::new(&mut meter, raw.len());
    let mut frame = HostRun::summarize(raw, 4, &mut ctx).unwrap();
    assert_eq!((frame.begin, frame.end, frame.effective_end), (4, 19, 16));
    assert_eq!(frame.suffix(raw, 4, &mut ctx).unwrap(), None);
    assert_eq!(frame.suffix(raw, 13, &mut ctx).unwrap(), Some(16));
    assert_eq!(
        (frame.begin, frame.end, frame.replay, frame.suffix_ordinal),
        (4, 19, 13, 3)
    );
    // A backwards replay request must reject, not silently rescan or invent a frame.
    assert_eq!(frame.suffix(raw, 4, &mut ctx), Err(Fault::Invalid));
}

#[test]
fn fixed_early_and_future_owner_selection() {
    use OwnerKind::{Code, Html, Link};
    use SelectedKind::Www;
    check("`www.a.com`", &[(Code, 0, 11, None, 1)], &[]);
    check(
        "[www.a.com/`x`](/u)",
        &[(Link, 0, 19, None, 2), (Code, 11, 14, Some(0), 2)],
        &[],
    );
    check(
        "www.a.com/`x`",
        &[(Code, 10, 13, None, 1)],
        &[(Www, 0, 13, 9)],
    );
    check(
        "www.a.com/<b>",
        &[(Html, 10, 13, None, 1)],
        &[(Www, 0, 10, 9)],
    );
    check(
        "www.a.com/[`x` b](/u)",
        &[(Link, 10, 21, None, 2), (Code, 11, 14, Some(0), 2)],
        &[(Www, 0, 14, 9)],
    );
}

#[test]
fn sweep_exclusive_end_and_inherited_liveness_are_not_lookahead() {
    let raw = "www.a.com/<b>";
    let mut meter = Meter::default();
    with_leaf(
        NativeNodeId::from_raw(1).unwrap(),
        BufferId::from_raw(1).unwrap(),
        0,
        raw.len(),
        |leaf| {
            let mut ctx = Context::new(&mut meter, raw.len());
            let mut sweep = Sweep::new(released(
                leaf,
                &[(OwnerKind::Html, 10, 13, None, 1)],
                &mut ctx,
            ));
            sweep.advance(raw, 0, None, &mut ctx).unwrap();
            let mut frame = HostRun::summarize(raw, 4, &mut ctx).unwrap();
            assert_eq!(frame.suffix(raw, 4, &mut ctx).unwrap(), Some(9));
            assert_eq!(path_end(raw, 9, &mut ctx).unwrap(), 10);
            assert_eq!((sweep.next, sweep.depth, sweep.live), (0, 0, 0));
            sweep.advance(raw, 10, Some(0), &mut ctx).unwrap();
            assert_eq!((sweep.next, sweep.depth, sweep.live), (0, 0, 0));
            sweep.advance(raw, 10, None, &mut ctx).unwrap();
            assert_eq!((sweep.next, sweep.depth, sweep.live), (1, 1, 1));
            drop(sweep);
            assert_eq!(ctx.host.live.get(), 0);
        },
    );
    // Abstract sweep transition on the same already-approved nested owner rows.
    // [0,11) is an injected consumed interval, NOT a claimed URL selection.
    let raw = "www.a.com/[`x` b](/u)";
    with_leaf(
        NativeNodeId::from_raw(1).unwrap(),
        BufferId::from_raw(1).unwrap(),
        0,
        raw.len(),
        |leaf| {
            let mut ctx = Context::new(&mut meter, raw.len());
            let mut sweep = Sweep::new(released(
                leaf,
                &[
                    (OwnerKind::Link, 10, 21, None, 2),
                    (OwnerKind::Code, 11, 14, Some(0), 2),
                ],
                &mut ctx,
            ));
            sweep.advance(raw, 11, Some(0), &mut ctx).unwrap();
            assert_eq!((sweep.next, sweep.live), (1, 0));
            sweep.advance(raw, 11, None, &mut ctx).unwrap();
            assert_eq!((sweep.next, sweep.depth, sweep.live), (2, 2, 0));
            assert!(!sweep.top(&mut ctx).unwrap().unwrap().active);
            drop(sweep);
            assert_eq!(ctx.host.live.get(), 0);
        },
    );
}

#[test]
fn malformed_ledger_rejects_and_releases() {
    let raw = "www.a.com/[`x` b](/u)";
    // Missing parent, falsely short subtree, crossing child. No grammar execution.
    for entries in [
        [
            (OwnerKind::Link, 10, 21, None, 2),
            (OwnerKind::Code, 11, 14, None, 2),
        ],
        [
            (OwnerKind::Link, 10, 21, None, 1),
            (OwnerKind::Code, 11, 14, Some(0), 2),
        ],
        [
            (OwnerKind::Link, 10, 14, None, 2),
            (OwnerKind::Code, 11, 21, Some(0), 2),
        ],
    ] {
        let mut meter = Meter::default();
        with_leaf(
            NativeNodeId::from_raw(1).unwrap(),
            BufferId::from_raw(1).unwrap(),
            0,
            raw.len(),
            |leaf| {
                let mut ctx = Context::new(&mut meter, raw.len());
                let shadow = released(leaf, &entries, &mut ctx);
                assert!(matches!(select(raw, shadow, &mut ctx), Err(Fault::Invalid)));
                assert_eq!(ctx.host.live.get(), 0);
                assert_eq!(ctx.host.permits.get(), 0);
            },
        );
    }
}

#[test]
fn selected_and_owner_scratch_release_on_first_fault() {
    let raw = "www.a.com/`x`";
    for fault in [Fault::Budget, Fault::Cancel] {
        let mut meter = Meter::default();
        with_leaf(
            NativeNodeId::from_raw(1).unwrap(),
            BufferId::from_raw(1).unwrap(),
            0,
            raw.len(),
            |leaf| {
                let mut ctx = Context::new(&mut meter, raw.len());
                let shadow = released(leaf, &[(OwnerKind::Code, 10, 13, None, 1)], &mut ctx);
                // Owner admission #1; selected #2 really allocated; stack #3 fails.
                ctx.host.fail_record = Some((3, fault));
                assert!(matches!(select(raw, shadow, &mut ctx), Err(error) if error == fault));
                assert_eq!(ctx.host.live.get(), 0);
                assert_eq!(ctx.host.permits.get(), 0);
                assert_eq!(ctx.host.reject(NativeViolation::InvalidProvenance), fault);
                assert_eq!(ctx.host.work(WorkDelta::Events(1)), Err(fault));
            },
        );
    }
    let mut meter = Meter::default();
    with_leaf(
        NativeNodeId::from_raw(1).unwrap(),
        BufferId::from_raw(1).unwrap(),
        0,
        raw.len(),
        |leaf| {
            let mut ctx = Context::new(&mut meter, raw.len());
            let shadow = released(leaf, &[(OwnerKind::Code, 10, 13, None, 1)], &mut ctx);
            // Owner directory/page already exist. Selected directory cannot reserve.
            ctx.host.fail_reserve = Some(3);
            assert!(matches!(select(raw, shadow, &mut ctx), Err(Fault::Budget)));
            assert_eq!(ctx.host.live.get(), 0);
            assert_eq!(ctx.host.permits.get(), 0);
        },
    );
}
