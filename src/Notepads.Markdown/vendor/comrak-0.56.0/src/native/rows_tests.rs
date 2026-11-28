// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    Resource,
    Cancel,
    Invalid,
    Invariant,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Call {
    Reserve,
    Actual,
    Copy,
    Other,
}
#[derive(Clone, Copy)]
enum Mode {
    Error,
    Cancel,
    Panic,
}
// Deliberately no Debug/Clone/Copy/Default bound on the permit.
struct Permit {
    bytes: u64,
    live: Rc<Cell<u64>>,
    alive: Rc<Cell<bool>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        assert!(self.alive.replace(false));
        self.live
            .set(self.live.get().checked_sub(self.bytes).unwrap());
    }
}
#[derive(Default)]
struct OwnerHost {
    live: Rc<Cell<u64>>,
    first: Option<Fault>,
    calls: Vec<Call>,
    fail_at: Option<(usize, Mode)>,
    peak: u64,
}
impl OwnerHost {
    fn call(&mut self, call: Call) -> Result<(), Fault> {
        if let Some(error) = self.first {
            return Err(error);
        }
        self.calls.push(call);
        if let Some((at, mode)) = self.fail_at
            && at == self.calls.len()
        {
            match mode {
                Mode::Error => {
                    self.first = Some(Fault::Resource);
                    return Err(Fault::Resource);
                }
                Mode::Cancel => {
                    self.first = Some(Fault::Cancel);
                    return Err(Fault::Cancel);
                }
                // The underlying successful operation has already happened;
                // the physical rows mutation guard must catch this unwind.
                Mode::Panic => panic!("after successful physical operation"),
            }
        }
        Ok(())
    }
}
impl Host for OwnerHost {
    type Error = Fault;
    type Lease = Permit;
    fn work(&mut self, delta: WorkDelta) -> Result<(), Fault> {
        self.call(if matches!(delta, WorkDelta::Copy(_)) {
            Call::Copy
        } else {
            Call::Other
        })
    }
    fn admit(&mut self, _: super::super::NativeAdmission) -> Result<(), Fault> {
        self.call(Call::Other)
    }
    fn reserve(&mut self, bytes: u64) -> Result<Permit, Fault> {
        if let Some(error) = self.first {
            return Err(error);
        }
        let permit = Permit {
            bytes,
            live: self.live.clone(),
            alive: Rc::new(Cell::new(true)),
        };
        self.live.set(self.live.get() + bytes);
        self.peak = self.peak.max(self.live.get());
        self.call(Call::Reserve)?;
        Ok(permit)
    }
    fn admit_actual(&mut self, permit: &mut Permit, bytes: u64) -> Result<(), Fault> {
        self.live.set(self.live.get() - permit.bytes + bytes);
        permit.bytes = bytes;
        self.peak = self.peak.max(self.live.get());
        self.call(Call::Actual)
    }
    fn fact(&mut self, _: super::super::AcceptedFact<'_>) -> Result<(), Fault> {
        panic!("rows do not transport facts")
    }
    fn reject(&mut self, violation: NativeViolation) -> Fault {
        *self
            .first
            .get_or_insert(if violation == NativeViolation::InternalInvariant {
                Fault::Invariant
            } else {
                Fault::Invalid
            })
    }
}
struct Record {
    text: String,
    padding: [u8; 2048],
    drops: Rc<Cell<usize>>,
    permit: Option<Rc<Cell<bool>>>,
}
impl Drop for Record {
    fn drop(&mut self) {
        if let Some(permit) = &self.permit {
            assert!(permit.get(), "row destroyed after its page permit");
        }
        self.drops.set(self.drops.get() + 1);
    }
}
fn append(
    rows: &mut NativeRows<Record, Permit>,
    host: &mut OwnerHost,
    drops: &Rc<Cell<usize>>,
    index: usize,
) -> Result<(), Fault> {
    rows.push(
        Record {
            text: format!("record {index}"),
            padding: [index as u8; 2048],
            drops: drops.clone(),
            permit: None,
        },
        host,
    )?;
    let page = rows.pages.last_mut().unwrap().as_mut().unwrap();
    page.values.last_mut().unwrap().permit = Some(page._lease.alive.clone());
    Ok(())
}

#[test]
fn ordered_noncopy_pages_keep_ordinals_through_two_directory_growths() {
    let mut host = OwnerHost::default();
    let drops = Rc::new(Cell::new(0));
    let mut rows = NativeRows::default();
    let width = NativeRows::<Record, Permit>::width();
    let count = width * 9 + 1;
    for index in 0..count {
        append(&mut rows, &mut host, &drops, index).unwrap();
    }
    assert_eq!(rows.pages.len(), 10);
    assert!(rows.pages.capacity() >= 16);
    for index in 0..count {
        let row = rows
            .get(index, |bytes| charge_host(&mut host, bytes))
            .unwrap()
            .unwrap();
        assert_eq!(row.text, format!("record {index}"));
        assert_eq!(row.padding[0], index as u8);
    }
    assert!(
        rows.get(count, |bytes| charge_host(&mut host, bytes))
            .unwrap()
            .is_none()
    );
    assert_eq!(drops.get(), 0);
    drop(rows);
    assert_eq!(drops.get(), count);
    assert_eq!(host.live.get(), 0);
}

#[test]
fn every_reserve_actual_and_descriptor_move_failure_keeps_unique_owners() {
    let count = NativeRows::<Record, Permit>::width() * 9 + 1;
    let mut reference = OwnerHost::default();
    let drops = Rc::new(Cell::new(0));
    let mut rows = NativeRows::default();
    for index in 0..count {
        append(&mut rows, &mut reference, &drops, index).unwrap();
    }
    let boundaries: Vec<_> = reference
        .calls
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            matches!(c, Call::Reserve | Call::Actual | Call::Copy).then_some(i + 1)
        })
        .collect();
    drop(rows);
    for mode in [Mode::Error, Mode::Cancel, Mode::Panic] {
        for &at in &boundaries {
            let drops = Rc::new(Cell::new(0));
            let mut rows = NativeRows::default();
            let mut host = OwnerHost {
                fail_at: Some((at, mode)),
                ..OwnerHost::default()
            };
            let mut created = 0;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for index in 0..count {
                    created += 1;
                    append(&mut rows, &mut host, &drops, index)?;
                }
                Ok::<_, Fault>(())
            }));
            match mode {
                Mode::Error => assert_eq!(outcome.unwrap(), Err(Fault::Resource)),
                Mode::Cancel => assert_eq!(outcome.unwrap(), Err(Fault::Cancel)),
                Mode::Panic => assert!(outcome.is_err()),
            }
            let expected = match mode {
                Mode::Error => Fault::Resource,
                Mode::Cancel => Fault::Cancel,
                Mode::Panic => Fault::Invariant,
            };
            assert_eq!(host.first, Some(expected));
            // A later cancel/error cannot replace the original physical failure.
            host.fail_at = Some((host.calls.len() + 1, Mode::Cancel));
            assert_eq!(host.work(WorkDelta::Events(1)), Err(expected));
            if let Some(pending) = &rows.pending {
                let old = rows.pages.iter().filter(|p| p.is_some()).count();
                let moved = pending.pages.iter().filter(|p| p.is_some()).count();
                assert_eq!(old + moved, rows.pages.len());
            }
            drop(rows);
            assert_eq!(drops.get(), created);
            assert_eq!(host.live.get(), 0, "boundary {at}");
        }
    }
}

#[test]
fn invalid_row_sizes_are_typed_before_allocation() {
    let mut host = OwnerHost::default();
    let mut zero = NativeRows::<(), Permit>::default();
    assert_eq!(zero.push((), &mut host), Err(Fault::Invalid));
    assert_eq!(host.live.get(), 0);
    let mut host = OwnerHost::default();
    let mut large = NativeRows::<[u8; 16_385], Permit>::default();
    assert_eq!(large.push([0; 16_385], &mut host), Err(Fault::Invalid));
    assert_eq!(host.live.get(), 0);
}

#[test]
fn repeated_cells_cross_pages_and_every_batch_failure_keeps_original_permits() {
    let count = NativeRows::<u32, Permit>::width() * 2 + 3;
    let mut host = OwnerHost::default();
    let mut rows = NativeRows::default();
    rows.extend_repeated(17_u32, count, &mut host).unwrap();
    let boundaries = host.calls.len();
    assert_eq!(rows.len(), count);
    let mut at = 0;
    while at < count {
        let page = rows
            .page_tail(at, |n| charge_host(&mut host, n))
            .unwrap()
            .unwrap();
        assert!(page.len() <= NativeRows::<u32, Permit>::width());
        assert!(page.iter().all(|n| *n == 17));
        at += page.len();
    }
    rows.extend_repeated(29, 11, &mut host).unwrap();
    assert_eq!(
        *rows
            .get(count - 1, |_| Ok::<_, Fault>(()))
            .unwrap()
            .unwrap(),
        17
    );
    assert_eq!(
        *rows.get(count, |_| Ok::<_, Fault>(())).unwrap().unwrap(),
        29
    );
    drop(rows);
    assert_eq!(host.live.get(), 0);
    for mode in [Mode::Error, Mode::Cancel, Mode::Panic] {
        for at in 1..=boundaries {
            let mut host = OwnerHost {
                fail_at: Some((at, mode)),
                ..OwnerHost::default()
            };
            let mut rows = NativeRows::default();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rows.extend_repeated(17_u32, count, &mut host)
            }));
            let expected = match mode {
                Mode::Error => {
                    assert_eq!(outcome.unwrap(), Err(Fault::Resource));
                    Fault::Resource
                }
                Mode::Cancel => {
                    assert_eq!(outcome.unwrap(), Err(Fault::Cancel));
                    Fault::Cancel
                }
                Mode::Panic => {
                    assert!(outcome.is_err());
                    Fault::Invariant
                }
            };
            assert_eq!(host.first, Some(expected));
            assert!(rows.len() < count);
            drop(rows);
            assert_eq!(host.live.get(), 0);
            assert_eq!(host.work(WorkDelta::Events(1)), Err(expected));
        }
    }
}
