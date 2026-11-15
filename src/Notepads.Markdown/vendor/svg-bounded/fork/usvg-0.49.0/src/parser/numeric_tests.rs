// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use markdown_svg_budget::{Backend, FailureKind, FirstFailureRef, LimitKind, Limits, Ticket};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

thread_local! {static NUMERIC_PHASE:std::cell::Cell<u8>=const{std::cell::Cell::new(0)};}
pub(super) struct NumericPhase(u8);
impl NumericPhase {
    pub(super) fn enter(role: u8) -> Self {
        Self(NUMERIC_PHASE.with(|p| p.replace(role)))
    }
}
impl Drop for NumericPhase {
    fn drop(&mut self) {
        NUMERIC_PHASE.with(|p| p.set(self.0));
    }
}
#[derive(Clone, Copy)]
struct NumericTicket {
    slot: u32,
    role: u8,
    bytes: usize,
    released: bool,
}
pub(crate) struct State {
    pub(super) used: AtomicUsize,
    pub(super) reserves: AtomicUsize,
    tickets: Mutex<Vec<NumericTicket>>,
    calls: AtomicUsize,
    injected: AtomicUsize,
    positive_calls: AtomicUsize,
    zero_calls: AtomicUsize,
    non_window_positive_calls: AtomicUsize,
    last_zero_call: AtomicUsize,
    byte_limit: AtomicUsize,
    reject_at: AtomicUsize,
    cancel_at: AtomicUsize,
    native_at: AtomicUsize,
    late: AtomicBool,
}
struct Ledger(Arc<State>);
impl Backend for Ledger {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let id = self.0.reserves.fetch_add(1, Ordering::SeqCst) + 1;
        if id == self.0.reject_at.load(Ordering::SeqCst) {
            return Err(Error::native(-911));
        }
        if self
            .0
            .used
            .load(Ordering::SeqCst)
            .checked_add(bytes)
            .filter(|n| *n <= self.0.byte_limit.load(Ordering::SeqCst))
            .is_none()
        {
            return Err(Error::new(FailureKind::Quota, LimitKind::Scratch));
        }
        self.0.used.fetch_add(bytes, Ordering::SeqCst);
        self.0.tickets.lock().unwrap().push(NumericTicket {
            slot: id as u32,
            role: NUMERIC_PHASE.with(|p| p.get()),
            bytes,
            released: false,
        });
        Ticket::new(bytes as u64, id as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        if self.0.late.load(Ordering::SeqCst) {
            return Err(Error::native(-912));
        }
        let bytes = ticket.account_nonce() as usize;
        let mut tickets = self.0.tickets.lock().unwrap();
        let event = tickets
            .iter_mut()
            .find(|e| e.slot == ticket.slot())
            .unwrap();
        assert!(!event.released);
        assert_eq!(event.bytes, bytes);
        event.released = true;
        self.0.used.fetch_sub(bytes, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let n = self.0.calls.fetch_add(1, Ordering::SeqCst);
        if work == 0 {
            self.0.zero_calls.fetch_add(1, Ordering::SeqCst);
            self.0.last_zero_call.store(n, Ordering::SeqCst);
        } else {
            self.0.positive_calls.fetch_add(1, Ordering::SeqCst);
            if work != 256 {
                self.0
                    .non_window_positive_calls
                    .fetch_add(1, Ordering::SeqCst);
            }
        }
        if n == self.0.cancel_at.load(Ordering::SeqCst) {
            self.0.injected.fetch_add(1, Ordering::SeqCst);
            return Err(Error::new(FailureKind::Cancelled, LimitKind::Work));
        }
        if n == self.0.native_at.load(Ordering::SeqCst) {
            self.0.injected.fetch_add(1, Ordering::SeqCst);
            return Err(Error::native(-914));
        }
        Ok(())
    }
}
pub(crate) fn request() -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        used: 0.into(),
        tickets: Mutex::new(Vec::new()),
        reserves: 0.into(),
        calls: 0.into(),
        injected: 0.into(),
        positive_calls: 0.into(),
        zero_calls: 0.into(),
        non_window_positive_calls: 0.into(),
        last_zero_call: usize::MAX.into(),
        byte_limit: usize::MAX.into(),
        reject_at: usize::MAX.into(),
        cancel_at: usize::MAX.into(),
        native_at: usize::MAX.into(),
        late: false.into(),
    });
    (
        ResourcesRef::new_prepaid(
            Ledger(state.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        state,
    )
}
fn options(r: &ResourcesRef) -> crate::Options<'static> {
    let mut opt = crate::Options::default();
    opt.processing = Arc::new(crate::ProcessingContext::with_resources(r.clone()));
    opt
}
fn first_table(group: &crate::Group) -> Option<&crate::NumberList> {
    for filter in group.filters() {
        for p in filter.primitives() {
            if let crate::filter::Kind::ComponentTransfer(v) = p.kind() {
                if let crate::filter::TransferFunction::Table(values) = v.func_r() {
                    return Some(values);
                }
            }
        }
    }
    group.children().iter().find_map(|n| {
        if let crate::Node::Group(g) = n {
            first_table(g)
        } else {
            None
        }
    })
}

#[test]
fn numeric_table_scene_clone_retains_one_paid_payload_until_both_readers_retire() {
    let (r, state) = request();
    let opt = options(&r);
    let xml=b"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><defs><filter id='f'><feComponentTransfer><feFuncR type='table' tableValues='0 .2 .7 1'/></feComponentTransfer></filter></defs><rect width='8' height='8' filter='url(#f)'/></svg>";
    let tree = crate::Tree::from_data_controlled(xml, &opt).unwrap();
    let pointer = first_table(tree.root()).unwrap().as_ptr();
    assert!(first_table(tree.root()).unwrap().belongs_to(&r));
    let reserves = state.reserves.load(Ordering::SeqCst);
    let numeric_before = state
        .tickets
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.role != 0)
        .copied()
        .collect::<Vec<_>>();
    assert!(numeric_before.iter().any(|e| e.role == 2 && !e.released));
    let clone = tree.try_clone_in(&opt.processing).unwrap();
    assert_eq!(first_table(clone.root()).unwrap().as_ptr(), pointer);
    let tickets = state.tickets.lock().unwrap();
    assert_eq!(
        tickets.iter().filter(|e| e.role != 0).count(),
        numeric_before.len(),
        "no numeric payload or frozen header is rebuilt"
    );
    assert!(
        state.reserves.load(Ordering::SeqCst) > reserves,
        "new scene metadata is charged separately"
    );
    drop(tickets);
    eprintln!(
        "numeric-owner initial-reserves={reserves} after-copy-reserves={} numeric-tickets={}",
        state.reserves.load(Ordering::SeqCst),
        numeric_before.len()
    );
    drop(tree);
    for original in numeric_before.iter().filter(|e| !e.released) {
        assert!(
            !state
                .tickets
                .lock()
                .unwrap()
                .iter()
                .find(|e| e.slot == original.slot)
                .unwrap()
                .released
        );
    }

    assert!(state.used.load(Ordering::SeqCst) > 0);
    assert_eq!(
        first_table(clone.root()).unwrap().as_slice(),
        &[0.0, 0.2, 0.7, 1.0]
    );
    drop((clone, opt));
    assert!(state.tickets.lock().unwrap().iter().all(|e| e.released));
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
}

#[test]
fn convolution_overflow_is_rejected_without_panic_or_silent_ready_and_fresh_recovers() {
    let xml=b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><filter id='f'><feConvolveMatrix order='2147483647 2147483647' kernelMatrix='1'/></filter></defs><rect width='8' height='8' filter='url(#f)'/></svg>";
    let (r, state) = request();
    let opt = options(&r);
    assert!(crate::Tree::from_data_controlled(xml, &opt).is_err());
    let error = r.failure().unwrap();
    assert_eq!(error.kind, FailureKind::Decode);
    assert_eq!(error.limit, LimitKind::Capacity);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
    let (fresh, _) = request();
    let opt = options(&fresh);
    let good=b"<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><filter id='f'><feConvolveMatrix order='1' kernelMatrix='1'/></filter></defs><rect width='8' height='8' filter='url(#f)'/></svg>";
    let tree = crate::Tree::from_data_controlled(good, &opt).unwrap();
    drop((tree, opt));
    fresh.validate_retired().unwrap();
}

#[test]
fn list_and_shared_header_reserve_faults_keep_original_native_error() {
    for point in 1..=2 {
        let (r, state) = request();
        state.reject_at.store(point, Ordering::SeqCst);
        let result = ListBuilder::filled(Some(&r), 8, 1.0f32).and_then(|v| v.freeze(Some(&r)));
        assert_eq!(result.unwrap_err(), Error::native(-911));
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn long_dash_validation_cancels_with_original_owner_intact_and_no_success() {
    let (r, state) = request();
    let values = ListBuilder::filled(Some(&r), 4096, 1.0f32)
        .unwrap()
        .freeze(Some(&r))
        .unwrap();
    let pointer = values.as_ptr();
    state
        .cancel_at
        .store(state.calls.load(Ordering::SeqCst) + 2, Ordering::SeqCst);
    let error = tiny_skia_path::StrokeDash::try_new_frozen(values.clone(), 1.0, &r).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values[4095], 1.0);
    drop(values);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
}

#[test]
fn numeric_header_inclusive_quota_boundary_and_late_release_keep_authoritative_failure() {
    let (r, state) = request();
    let values = ListBuilder::filled(Some(&r), 8, 1.0f32)
        .unwrap()
        .freeze(Some(&r))
        .unwrap();
    let exact = state.used.load(Ordering::SeqCst);
    drop(values);
    r.validate_retired().unwrap();
    for (delta, success) in [(-1isize, false), (0, true), (1, true)] {
        let (r, state) = request();
        state
            .byte_limit
            .store(exact.checked_add_signed(delta).unwrap(), Ordering::SeqCst);
        let values = ListBuilder::filled(Some(&r), 8, 1.0f32).and_then(|v| v.freeze(Some(&r)));
        assert_eq!(values.is_ok(), success);
        if !success {
            assert_eq!(
                r.failure(),
                Some(Error::new(FailureKind::Quota, LimitKind::Scratch))
            );
        }
        drop(values);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
    let (r, state) = request();
    let values = ListBuilder::filled(Some(&r), 8, 1.0f32)
        .unwrap()
        .freeze(Some(&r))
        .unwrap();
    let original = Error::new(FailureKind::Cancelled, LimitKind::Work);
    r.record(original);
    state.late.store(true, Ordering::SeqCst);
    drop(values);
    assert_eq!(r.failure(), Some(original));
    assert!(state.used.load(Ordering::SeqCst) > 0);
    assert!(r.statistics().live_leases > 0);
    assert!(r.validate_retired().is_err());
}

#[test]
fn frozen_dash_rejects_compat_and_other_ledger_without_erasing_owner() {
    let (r, state) = request();
    let (other, _) = request();
    let values = ListBuilder::filled(Some(&r), 8, 1.0f32)
        .unwrap()
        .freeze(Some(&r))
        .unwrap();
    let pointer = values.as_ptr();
    assert_eq!(
        tiny_skia_path::StrokeDash::try_new_frozen(values.clone(), 0.0, &other)
            .unwrap_err()
            .kind,
        FailureKind::Invariant
    );
    assert_eq!(values.as_ptr(), pointer);
    assert!(r.failure().is_none());
    drop(values);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
    other.validate_retired().unwrap();
    let (fresh, _) = request();
    assert_eq!(
        tiny_skia_path::StrokeDash::try_new_frozen(vec![1.0, 1.0].into(), 0.0, &fresh)
            .unwrap_err()
            .kind,
        FailureKind::Invariant
    );
    fresh.validate_retired().unwrap();
}

fn parse_numeric_list(
    input: &str,
    r: &ResourcesRef,
    state: &State,
    prepare: impl FnOnce(usize, usize),
) -> (Option<crate::NumberList>, usize, usize, u64) {
    use crate::parser::svgtree::{AId, Document, FromValue};
    let xml = roxmltree::Document::parse("<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    let doc = Document::parse_tree_for_source(
        &xml,
        None,
        crate::parser::source_kind::SourceKind::GeneralImage,
        Some(r.clone()),
    )
    .unwrap();
    let calls = state.calls.load(Ordering::SeqCst);
    let reserves = state.reserves.load(Ordering::SeqCst);
    let work = r.statistics().work;
    prepare(calls, reserves);
    let values = <crate::NumberList as FromValue>::parse(doc.root(), AId::Values, input);
    let calls = state.calls.load(Ordering::SeqCst) - calls;
    let reserves = state.reserves.load(Ordering::SeqCst) - reserves;
    let work = r.statistics().work - work;
    drop((doc, xml));
    (values, calls, reserves, work)
}

#[test]
fn real_number_parser_keeps_full_tail_validation_and_original_float_values() {
    for input in ["", "1,2,3", "+1 -.2 3e2 -0 1e40", "1 2 garbage 3", "1e9999"] {
        let (r, state) = request();
        let (actual, _, _, _) = parse_numeric_list(input, &r, &state, |_, _| {});
        let expected: Option<Vec<_>> = svgtypes::NumberListParser::from(input)
            .map(|v| v.ok().map(|v| (v as f32).to_bits()))
            .collect();
        assert_eq!(
            actual
                .as_ref()
                .map(|v| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
            expected,
            "{input}"
        );
        drop(actual);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn long_real_numeric_parse_charges_work_and_all_actual_cancel_and_reserve_points_retire() {
    let source = ".25 ".repeat(10000);
    let (r, state) = request();
    let (values, calls, reserves, work) = parse_numeric_list(&source, &r, &state, |_, _| {});
    let values = values.unwrap();
    assert_eq!(values.len(), 10000);
    assert!(values.belongs_to(&r));
    assert!(values.iter().all(|v| *v == 0.25));
    assert!(calls > 40 && reserves > 2 && work >= 10000);
    drop(values);
    r.validate_retired().unwrap();
    for point in 0..calls {
        let (r, state) = request();
        let (values, _, _, _) = parse_numeric_list(&source, &r, &state, |base, _| {
            state.cancel_at.store(base + point, Ordering::SeqCst)
        });
        assert!(values.is_none(), "cancel point {point}/{calls}");
        assert_eq!(r.failure().unwrap().kind, FailureKind::Cancelled);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
    for point in 1..=reserves {
        let (r, state) = request();
        let (values, _, _, _) = parse_numeric_list(&source, &r, &state, |_, base| {
            state.reject_at.store(base + point, Ordering::SeqCst)
        });
        assert!(values.is_none(), "reserve point {point}/{reserves}");
        assert_eq!(r.failure(), Some(Error::native(-911)));
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
    eprintln!(
        "numeric_actual tokens=10000 bytes={} work={work} cancel_points={calls} reserve_points={reserves}",
        source.len()
    );
}

#[test]
fn all_three_real_dash_validation_loops_charge_and_cancel_without_losing_the_array() {
    let (r, state) = request();
    let values = ListBuilder::filled(Some(&r), 4096, 1.0f32)
        .unwrap()
        .freeze(Some(&r))
        .unwrap();
    let before = state.calls.load(Ordering::SeqCst);
    let positive_before = state.positive_calls.load(Ordering::SeqCst);
    let zero_before = state.zero_calls.load(Ordering::SeqCst);
    let non_window_before = state.non_window_positive_calls.load(Ordering::SeqCst);
    let work = r.statistics().work;
    let dash = tiny_skia_path::StrokeDash::try_new_frozen(values.clone(), 4095.0, &r)
        .unwrap()
        .unwrap();
    let calls = state.calls.load(Ordering::SeqCst) - before;
    let work = r.statistics().work - work;
    // Keep all three original 4096/256 validation scans. The new mandatory
    // successful exit check has zero work and is a separate cancellation point.
    assert_eq!(
        state.positive_calls.load(Ordering::SeqCst) - positive_before,
        48
    );
    assert_eq!(
        state.non_window_positive_calls.load(Ordering::SeqCst) - non_window_before,
        0
    );
    assert_eq!(state.zero_calls.load(Ordering::SeqCst) - zero_before, 1);
    assert_eq!(calls, 48 + 1);
    assert_eq!(
        state.last_zero_call.load(Ordering::SeqCst),
        before + calls - 1
    );
    assert_eq!(work, 4096 * 3);
    drop((dash, values));
    r.validate_retired().unwrap();
    for point in 0..calls {
        let (r, state) = request();
        let values = ListBuilder::filled(Some(&r), 4096, 1.0f32)
            .unwrap()
            .freeze(Some(&r))
            .unwrap();
        let pointer = values.as_ptr();
        state
            .cancel_at
            .store(state.calls.load(Ordering::SeqCst) + point, Ordering::SeqCst);
        let result = tiny_skia_path::StrokeDash::try_new_frozen(values.clone(), 4095.0, &r);
        assert_eq!(result.unwrap_err().kind, FailureKind::Cancelled);
        assert_eq!(values.as_ptr(), pointer);
        assert_eq!(values[4095], 1.0);
        drop(values);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
    eprintln!(
        "dash_actual values=4096 work={work} scan_windows=48 tail_zero_checks=1 cancel_points={calls} phases=negative,sum,first_interval"
    );
}

#[test]
fn existing_first_failure_wins_even_for_empty_static_dash() {
    let (r, _) = request();
    let first = Error::native(-913);
    r.record(first);
    assert_eq!(
        tiny_skia_path::StrokeDash::try_new_frozen(crate::NumberList::from_static(&[]), 0.0, &r)
            .unwrap_err(),
        first
    );
    r.validate_retired().unwrap();
}

#[test]
fn controlled_number_lists_keep_independent_long_numeric_and_whitespace_values() {
    for count in [4095, 4096, 4097, 65535, 65536, 65537, 131073] {
        // These values have independent exact or underflow-to-zero answers;
        // the ordinary parser is deliberately not the expected-value oracle.
        for (source, expected) in [
            (format!("0.{}1 1", "0".repeat(count)), [0, 1.0f32.to_bits()]),
            (format!("1.{} 1", "0".repeat(count)), [1.0f32.to_bits(); 2]),
            (format!("{}1 1", "0".repeat(count)), [1.0f32.to_bits(); 2]),
            (format!("1e{} 1", "0".repeat(count)), [1.0f32.to_bits(); 2]),
            (
                format!("{}1 1{}", " ".repeat(count), " ".repeat(count)),
                [1.0f32.to_bits(); 2],
            ),
        ] {
            let (r, state) = request();
            let (values, _, _, _) = parse_numeric_list(&source, &r, &state, |_, _| {});
            let values = values.unwrap();
            assert_eq!(
                values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
            );
            assert!(values.belongs_to(&r));
            drop(values);
            assert_eq!(state.used.load(Ordering::SeqCst), 0);
            r.validate_retired().unwrap();
        }
    }
}

#[test]
fn controlled_number_list_event_fees_and_shared_scan_credit_do_not_round_per_token() {
    for count in [0usize, 255, 256, 257] {
        let source = "1 ".repeat(count);
        // The same request-wide credit also pays existing BudgetVec growth
        // copies. Measure that independent value-storage workload; neither the
        // source lexer nor this test may treat those bytes as free credit.
        let (storage_resources, storage_state) = request();
        let storage_credit = storage_resources.scan_allowance().unwrap();
        let mut storage = ListBuilder::<f32>::new(Some(&storage_resources));
        for _ in 0..count {
            storage.push(1.0).unwrap();
        }
        let storage_scan_bytes = storage_credit - storage_resources.scan_allowance().unwrap();
        drop(storage);
        assert_eq!(storage_state.used.load(Ordering::SeqCst), 0);
        storage_resources.validate_retired().unwrap();
        let (r, state) = request();
        // FrozenList::try_freeze uses the existing PaidShared header, whose
        // try_new_with path has its own checkpoint. Measure that same typed
        // empty header separately; it is not part of list-event/scan work.
        let before_freeze = r.statistics().work;
        let header = ListBuilder::<f32>::new(Some(&r)).freeze(Some(&r)).unwrap();
        let freeze_work = r.statistics().work - before_freeze;
        drop(header);
        let mut preparation_work = 0;
        let (values, _, _, work) = parse_numeric_list(&source, &r, &state, |_, _| {
            let before = r.statistics().work;
            let credit = r.scan_allowance().unwrap();
            r.consume_scan_work(credit - 1).unwrap();
            assert_eq!(r.scan_allowance().unwrap(), 1);
            preparation_work = r.statistics().work - before;
        });
        let values = values.unwrap();
        assert_eq!(values.len(), count);
        // Exclude only the measured preparation callback. The actual parser
        // then pays the original event count and one shared scan replenishment.
        let expected_events = (count / 256 + 1) as u64 * 256;
        let expected_scan = if count == 0 { 0 } else { 65_536 };
        assert_eq!(
            work - preparation_work - freeze_work,
            expected_events + expected_scan
        );
        if count != 0 {
            assert_eq!(
                r.scan_allowance().unwrap(),
                // The lexical source pass and every complete one-byte f64 token
                // share the same credit; each of the count conversions costs 1.
                65_537 - source.len() - storage_scan_bytes - count
            );
        }
        drop(values);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
}

#[test]
fn controlled_number_list_one_credit_replenishment_keeps_cancel_native_and_work_faults() {
    let source = format!("1.{} 1", "0".repeat(131073));
    for mode in 0..3 {
        let (r, state) = request();
        let (values, _, _, _) = parse_numeric_list(&source, &r, &state, |_, _| {
            let credit = r.scan_allowance().unwrap();
            r.consume_scan_work(credit - 1).unwrap();
            assert_eq!(r.scan_allowance().unwrap(), 1);
            let next = state.calls.load(Ordering::SeqCst);
            match mode {
                0 => state.cancel_at.store(next + 1, Ordering::SeqCst),
                1 => state.native_at.store(next + 1, Ordering::SeqCst),
                _ => r
                    .checkpoint(Limits::default().work - r.statistics().work - 256)
                    .unwrap(),
            }
        });
        assert!(values.is_none());
        let expected = match mode {
            0 => Error::new(FailureKind::Cancelled, LimitKind::Work),
            1 => Error::native(-914),
            _ => Error::new(FailureKind::Quota, LimitKind::Work),
        };
        assert_eq!(r.failure(), Some(expected));
        assert_eq!(r.record(Error::native(-999)), expected);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
}

fn long_numeric_filter(count: usize) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><filter id='f'><feComponentTransfer><feFuncR type='table' tableValues='0.{}1 1'/></feComponentTransfer></filter></defs><rect width='8' height='8' fill='red' filter='url(#f)'/></svg>",
        "0".repeat(count)
    )
}

fn long_numeric_dash(count: usize, tail: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><path d='M0 4H8' stroke='red' fill='none' stroke-dasharray='1.{}px 2{}'/></svg>",
        "0".repeat(count),
        tail
    )
}

fn first_dash(group: &crate::Group) -> Option<&[f32]> {
    group.children().iter().find_map(|node| match node {
        crate::Node::Path(path) => path.stroke().and_then(crate::Stroke::dasharray),
        crate::Node::Group(child) => first_dash(child),
        _ => None,
    })
}

#[test]
fn real_controlled_tree_retains_long_filter_and_length_values_and_syntax_prefix() {
    for (source, filter) in [
        (long_numeric_filter(131073), true),
        (long_numeric_dash(131073, ""), false),
        (long_numeric_dash(131073, " garbage"), false),
        (long_numeric_dash(131073, " 中"), false),
    ] {
        let (r, state) = request();
        let opt = options(&r);
        let tree = crate::Tree::from_data_controlled(source.as_bytes(), &opt).unwrap();
        if filter {
            let values = first_table(tree.root()).unwrap();
            assert!(values.belongs_to(&r));
            assert_eq!(
                values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                [0, 1.0f32.to_bits()]
            );
        } else {
            assert_eq!(first_dash(tree.root()).unwrap(), &[1.0, 2.0]);
        }
        drop((tree, opt));
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
}

// Pure borrowed observations only: never run a validator/serializer after the
// return snapshot, because that would move the injection outside the Tree call.
fn assert_original_long_fixture(tree: &crate::Tree, r: &ResourcesRef, filter: bool) {
    assert!(
        tree.processing_context()
            .resources()
            .is_some_and(|source| source.is_same_request(r))
    );
    assert_eq!((tree.size().width(), tree.size().height()), (8.0, 8.0));
    if filter {
        let values = first_table(tree.root()).unwrap();
        assert!(values.belongs_to(r));
        assert_eq!(values.len(), 2);
        assert_eq!(
            [values[0].to_bits(), values[1].to_bits()],
            [0, 1.0f32.to_bits()]
        );
    } else {
        fn path(group: &crate::Group) -> Option<&crate::Path> {
            group.children().iter().find_map(|node| match node {
                crate::Node::Path(p) if p.stroke().and_then(crate::Stroke::dasharray).is_some() => {
                    Some(p.as_ref())
                }
                crate::Node::Group(g) => path(g),
                _ => None,
            })
        }
        let path = path(tree.root()).unwrap();
        let values = path.stroke().unwrap().dasharray.as_ref().unwrap();
        assert!(values.belongs_to(r));
        assert_eq!(values.as_slice(), &[1.0, 2.0]);
        assert!(path.data.belongs_to(r));
        assert!(path.data().belongs_to(r));
        assert_eq!(
            path.data()
                .points()
                .iter()
                .map(|p| [p.x.to_bits(), p.y.to_bits()])
                .collect::<Vec<_>>(),
            vec![
                [0.0f32.to_bits(), 4.0f32.to_bits()],
                [8.0f32.to_bits(), 4.0f32.to_bits()]
            ]
        );
    }
}

#[test]
fn every_actual_long_filter_and_dash_checkpoint_fault_returns_no_partial_tree() {
    let mut total_attempted = 0;
    let mut total_injected = 0;
    let mut total_not_reached = 0;
    for (source_index, source) in [
        long_numeric_filter(65537),
        long_numeric_dash(65537, " garbage"),
    ]
    .into_iter()
    .enumerate()
    {
        let (r, state) = request();
        let opt = options(&r);
        let tree = crate::Tree::from_data_controlled(source.as_bytes(), &opt).unwrap();
        let calibration_injected = state.injected.load(Ordering::SeqCst);
        let calls = state.calls.load(Ordering::SeqCst);
        let calibration_resource = r.failure();
        let calibration_processing = opt.processing.resource_failure();
        assert_eq!(calibration_injected, 0);
        assert!(calls > 4);
        assert_eq!(calibration_resource, None);
        assert_eq!(calibration_processing, None);
        assert_original_long_fixture(&tree, &r, source_index == 0);
        assert_eq!(state.calls.load(Ordering::SeqCst), calls);
        drop((tree, opt));
        assert_eq!(r.failure(), None);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
        eprintln!(
            "LONG_NUMERIC_CALIBRATION source={source_index} calls={calls} injected=0 resource=None processing=None retired=true"
        );
        let mut transports = [0usize; 3];
        let mut attempted = 0;
        let mut injected_count = 0;
        let mut not_reached = 0;
        for native in [false, true] {
            let mut error_injected = 0;
            let mut error_not_reached = 0;
            for point in 0..calls {
                let (r, state) = request();
                let opt = options(&r);
                if native {
                    state.native_at.store(point, Ordering::SeqCst);
                } else {
                    state.cancel_at.store(point, Ordering::SeqCst);
                }
                let result = crate::Tree::from_data_controlled(source.as_bytes(), &opt);
                // Freeze all interpretations at the actual return boundary,
                // before value checks, reporting helpers or physical cleanup.
                let injected = state.injected.load(Ordering::SeqCst);
                let actual_calls = state.calls.load(Ordering::SeqCst);
                let resource_at_return = r.failure();
                let processing_at_return = opt.processing.resource_failure();
                let tree_ok = result.is_ok();
                let error = if native {
                    Error::native(-914)
                } else {
                    Error::new(FailureKind::Cancelled, LimitKind::Work)
                };
                attempted += 1;
                assert!(
                    injected <= 1,
                    "source={source_index} native={native} target={point}: repeated backend fault"
                );
                if point == 0 {
                    assert_eq!(
                        injected, 1,
                        "each input/error calibration target zero must actually inject"
                    );
                }
                let status;
                if injected == 1 {
                    status = "INJECTED";
                    injected_count += 1;
                    error_injected += 1;
                    let (actual, transport) = match result {
                        Err(crate::Error::ParsingFailed(roxmltree::Error::Resource(actual))) => {
                            (actual, 0)
                        }
                        Err(crate::Error::Resource(actual)) => (actual, 1),
                        Err(crate::Error::ProcessingFailure(receipt)) => (receipt.typed_failure, 2),
                        other => panic!(
                            "injected point {point}/{calls}, native={native}, actual={:?}",
                            other.as_ref().err()
                        ),
                    };
                    assert_eq!(actual, error, "point {point}/{calls}, native={native}");
                    transports[transport] += 1;
                    assert_eq!(resource_at_return, Some(error));
                    assert_eq!(processing_at_return, Some(error));
                    drop(opt);
                    assert_eq!(r.failure(), Some(error));
                    assert_eq!(state.used.load(Ordering::SeqCst), 0);
                    r.validate_retired().unwrap();
                } else {
                    status = "NOT_EXERCISED";
                    not_reached += 1;
                    error_not_reached += 1;
                    assert!(actual_calls <= point);
                    assert_eq!(resource_at_return, None);
                    assert_eq!(processing_at_return, None);
                    let tree = result.expect(
                        "unreached target still requires a complete correct successful Tree",
                    );
                    assert_original_long_fixture(&tree, &r, source_index == 0);
                    assert_eq!(state.injected.load(Ordering::SeqCst), 0);
                    assert_eq!(state.calls.load(Ordering::SeqCst), actual_calls);
                    drop((tree, opt));
                    assert_eq!(r.failure(), None);
                    assert_eq!(state.used.load(Ordering::SeqCst), 0);
                    r.validate_retired().unwrap();
                }
                eprintln!(
                    "LONG_NUMERIC_CASE source={source_index} native={native} reference={calls} target={point} injected={injected} actualcalls={actual_calls} resource={resource_at_return:?} processing={processing_at_return:?} tree_ok={tree_ok} retired=true status={status}"
                );
            }
            assert!(error_injected > 0);
            eprintln!(
                "LONG_NUMERIC_ERROR source={source_index} native={native} attempted={calls} injected={error_injected} not_reached={error_not_reached}"
            );
        }
        assert_eq!(attempted, calls * 2);
        assert_eq!(attempted, injected_count + not_reached);
        assert!(transports.iter().all(|n| *n > 0));
        total_attempted += attempted;
        total_injected += injected_count;
        total_not_reached += not_reached;
        eprintln!(
            "LONG_NUMERIC_SOURCE source={source_index} reference={calls} attempted={attempted} injected={injected_count} not_reached={not_reached} xml_resource={} direct_resource={} processing_receipt={} reference_window_exercised={} original_matrix_complete=false",
            transports[0],
            transports[1],
            transports[2],
            not_reached == 0
        );
    }
    assert_eq!(total_attempted, total_injected + total_not_reached);
    eprintln!(
        "LONG_NUMERIC_TOTAL attempted={total_attempted} injected={total_injected} not_reached={total_not_reached} legacy_run_attribution=UNKNOWN original_matrix_complete=false"
    );
}
