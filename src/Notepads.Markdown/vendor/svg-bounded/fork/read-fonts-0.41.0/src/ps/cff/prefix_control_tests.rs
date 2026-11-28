// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real DICT -> Stack prefix work: these tests precede the production repair.
use super::*;
use crate::ps::{
    cff::dict,
    hinting::{Blues, StemSnaps},
};
use markdown_svg_budget::{
    Backend, Error as ControlError, FailureKind, FirstFailureRef, LimitKind, Limits,
    RequestControl, ResourceKind, ResourcesRef, Ticket,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Events {
    work: Vec<u64>,
    total: u64,
    cancel_after: Option<u64>,
}
struct Host(Arc<Mutex<Events>>);
impl Backend for Host {
    fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, ControlError> {
        panic!("prefix decoding must not allocate a paid buffer")
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ControlError> {
        panic!("prefix decoding must not commit a buffer")
    }
    fn release_after_free(&self, _: Ticket) -> Result<(), ControlError> {
        panic!("prefix decoding must not release a buffer")
    }
    fn checkpoint(&self, work: u64) -> Result<(), ControlError> {
        let mut e = self.0.lock().unwrap();
        e.work.push(work);
        e.total += work;
        if e.cancel_after.is_some_and(|n| e.total > n) {
            Err(cancelled())
        } else {
            Ok(())
        }
    }
}
fn cancelled() -> ControlError {
    ControlError::new(FailureKind::Cancelled, LimitKind::None)
}
fn quota() -> ControlError {
    ControlError::new(FailureKind::Quota, LimitKind::Work)
}
fn resources(work: u64, cancel_after: Option<u64>) -> (ResourcesRef, Arc<Mutex<Events>>) {
    let events = Arc::new(Mutex::new(Events {
        cancel_after,
        ..Default::default()
    }));
    let limits = Limits {
        work,
        ..Limits::default()
    };
    (
        ResourcesRef::new_prepaid(Host(events.clone()), FirstFailureRef::new_prepaid(), limits),
        events,
    )
}
fn input_values() -> Vec<i32> {
    (0..513)
        .map(|i| [32767, 32767, -32768, 1, 0, -1][i % 6])
        .collect()
}
fn dictionary(op: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in input_values() {
        bytes.push(29);
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend(op);
    bytes.extend([162, 17]);
    bytes
}
fn expected(op: &[u8]) -> dict::Entry {
    let mut sum = Fixed::ZERO;
    let values: Vec<_> = input_values()
        .into_iter()
        .map(|n| {
            sum = sum.wrapping_add(Fixed::from_i32(n));
            sum
        })
        .collect();
    match op {
        [6] => dict::Entry::BlueValues(Blues::new(values.into_iter())),
        [7] => dict::Entry::OtherBlues(Blues::new(values.into_iter())),
        [8] => dict::Entry::FamilyBlues(Blues::new(values.into_iter())),
        [9] => dict::Entry::FamilyOtherBlues(Blues::new(values.into_iter())),
        [12, 12] => dict::Entry::StemSnapH(StemSnaps::new(values.into_iter())),
        [12, 13] => dict::Entry::StemSnapV(StemSnaps::new(values.into_iter())),
        _ => unreachable!(),
    }
}
fn prefix_case(op: &[u8]) {
    let bytes = dictionary(op);
    let value = expected(op);
    let mut ordinary = dict::entries(&bytes, None);
    assert_eq!(ordinary.next().unwrap().unwrap(), value);
    assert_eq!(
        ordinary.next().unwrap().unwrap(),
        dict::Entry::CharstringsOffset(23)
    );
    assert!(ordinary.next().is_none());
    let (r, _) = resources(u64::MAX, None);
    let mut cursor = dict::try_entries(&bytes, None);
    assert_eq!(
        cursor
            .try_next(&mut RequestControl(&r))
            .unwrap()
            .unwrap()
            .unwrap(),
        value
    );
    assert_eq!(
        cursor
            .try_next(&mut RequestControl(&r))
            .unwrap()
            .unwrap()
            .unwrap(),
        dict::Entry::CharstringsOffset(23)
    );
    assert!(cursor.try_next(&mut RequestControl(&r)).unwrap().is_none());
    // Xuid consumes the exact same513 wire operands without a prefix operation.
    // Measure its actual token work instead of copying parser checkpoint counts.
    let calibration = dictionary(&[14]);
    let (r, _) = resources(u64::MAX, None);
    assert!(matches!(
        dict::try_entries(&calibration, None)
            .try_next(&mut RequestControl(&r))
            .unwrap()
            .unwrap()
            .unwrap(),
        dict::Entry::Xuid
    ));
    let token_work = r.statistics().work;
    for cancel in [false, true] {
        let cap = token_work + 256;
        let (r, events) = if cancel {
            resources(u64::MAX, Some(cap))
        } else {
            resources(cap, None)
        };
        let mut cursor = dict::try_entries(&bytes, None);
        let error = if cancel { cancelled() } else { quota() };
        let result = cursor.try_next(&mut RequestControl(&r));
        println!(
            "prefix {:?} cancel={} token_work={} charged={} candidate={} fault={:?}",
            op,
            cancel,
            token_work,
            r.statistics().work,
            result.is_ok(),
            r.failure()
        );
        assert_eq!(result.err(), Some(error));
        assert_eq!(r.failure(), Some(error));
        let before = events.lock().unwrap().work.clone();
        assert_eq!(cursor.try_next(&mut RequestControl(&r)).err(), Some(error));
        assert_eq!(events.lock().unwrap().work, before);
        assert_eq!(r.statistics().live_leases, 0);
    }
}
macro_rules! case {
    ($name:ident,$bytes:expr) => {
        #[test]
        fn $name() {
            prefix_case($bytes)
        }
    };
}
case!(blue_values_stops_before_operand257, &[6]);
case!(other_blues_stops_before_operand257, &[7]);
case!(family_blues_stops_before_operand257, &[8]);
case!(family_other_blues_stops_before_operand257, &[9]);
case!(stem_snap_h_stops_before_operand257, &[12, 12]);
case!(stem_snap_v_stops_before_operand257, &[12, 13]);

#[test]
fn stack_mixed513_rejects_before_reading_or_writing_operand257() {
    let numbers: Vec<_> = (0..513)
        .map(|i| match i % 4 {
            0 => Number::Fixed(Fixed::from_bits(i32::MAX)),
            1 => Number::I32(32767),
            2 => Number::Fixed(Fixed::from_bits(i32::MIN)),
            _ => Number::I32(-32768),
        })
        .collect();
    let mut sum = Fixed::ZERO;
    let full: Vec<_> = numbers
        .iter()
        .map(|n| {
            let value = match *n {
                Number::Fixed(value) => value,
                Number::I32(value) => Fixed::from_i32(value),
            };
            sum = sum.wrapping_add(value);
            Number::Fixed(sum)
        })
        .collect();
    let make = || {
        let mut stack = Stack::new();
        for &number in &numbers {
            stack.push(number).unwrap();
        }
        stack
    };
    let mut ordinary = make();
    ordinary.apply_delta_prefix_sum();
    assert_eq!(ordinary.number_values().collect::<Vec<_>>(), full);
    assert_eq!(ordinary.prefix_operand_steps, 513);
    let (r, _) = resources(u64::MAX, None);
    let mut controlled = make();
    controlled
        .try_apply_delta_prefix_sum(&mut RequestControl(&r))
        .unwrap();
    assert_eq!(controlled.number_values().collect::<Vec<_>>(), full);
    assert_eq!(controlled.prefix_operand_steps, 513);
    assert_eq!(r.statistics().work, 513);
    for cancel in [false, true] {
        let (r, events) = if cancel {
            resources(u64::MAX, Some(256))
        } else {
            resources(256, None)
        };
        let mut stack = make();
        let error = if cancel { cancelled() } else { quota() };
        assert_eq!(
            stack.try_apply_delta_prefix_sum(&mut RequestControl(&r)),
            Err(error)
        );
        assert_eq!(stack.prefix_operand_steps, 256);
        let actual = stack.number_values().collect::<Vec<_>>();
        assert_eq!(&actual[..256], &full[..256]);
        assert_eq!(&actual[256..], &numbers[256..]);
        let calls = events.lock().unwrap().work.clone();
        assert_eq!(
            stack.try_apply_delta_prefix_sum(&mut RequestControl(&r)),
            Err(error)
        );
        assert_eq!(stack.prefix_operand_steps, 256);
        assert_eq!(stack.number_values().collect::<Vec<_>>(), actual);
        assert_eq!(events.lock().unwrap().work, calls);
    }
}
#[test]
fn empty_and_single_prefix_preserve_original_number_representation() {
    for numbers in [
        vec![],
        vec![Number::I32(32767)],
        vec![Number::Fixed(Fixed::from_bits(i32::MAX))],
    ] {
        let mut stack = Stack::new();
        for &n in &numbers {
            stack.push(n).unwrap();
        }
        let (r, _) = resources(0, None);
        stack
            .try_apply_delta_prefix_sum(&mut RequestControl(&r))
            .unwrap();
        assert_eq!(stack.number_values().collect::<Vec<_>>(), numbers);
        assert_eq!(stack.prefix_operand_steps, 0);
        assert_eq!(r.statistics().work, 0);
    }
}
