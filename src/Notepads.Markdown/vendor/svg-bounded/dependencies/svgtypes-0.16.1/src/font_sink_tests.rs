// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Public sink behavior and natural paid Name cleanup; no unsafe test hooks.
extern crate std;

use super::*;
use markdown_svg_budget::{
    Backend, Error as BudgetError, FirstFailureRef, Limits, PaidUtf8, ResourceKind, ResourcesRef,
    Ticket,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};

#[derive(Clone, Copy, Debug)]
struct StackName {
    bytes: [u8; 128],
    len: usize,
}
impl AsRef<str> for StackName {
    fn as_ref(&self) -> &str {
        // Fixed 128-byte test storage bounds even this safe validation by a
        // constant. Production TreeString/PaidUtf8 borrows directly in O(1).
        core::str::from_utf8(&self.bytes[..self.len]).unwrap()
    }
}
#[derive(Clone, Copy, Debug)]
struct StackSink {
    names: [Option<(FontFamilyKind, StackName)>; 16],
    len: usize,
    starts: usize,
    writes: usize,
    reject_write: usize,
}
impl StackSink {
    fn new() -> Self {
        Self {
            names: [None; 16],
            len: 0,
            starts: 0,
            writes: 0,
            reject_write: usize::MAX,
        }
    }
    fn logical(&self) -> Vec<FontFamily> {
        self.names[..self.len]
            .iter()
            .map(|item| {
                let (kind, name) = item.as_ref().unwrap();
                match kind {
                    FontFamilyKind::Serif => FontFamily::Serif,
                    FontFamilyKind::SansSerif => FontFamily::SansSerif,
                    FontFamilyKind::Cursive => FontFamily::Cursive,
                    FontFamilyKind::Fantasy => FontFamily::Fantasy,
                    FontFamilyKind::Monospace => FontFamily::Monospace,
                    FontFamilyKind::Named => FontFamily::Named(name.as_ref().to_string()),
                }
            })
            .collect()
    }
}
impl FontFamilySink<i32> for StackSink {
    type Name = StackName;
    fn new_name(&mut self) -> Result<StackName, i32> {
        self.starts += 1;
        Ok(StackName {
            bytes: [0; 128],
            len: 0,
        })
    }
    fn push_char(&mut self, name: &mut StackName, ch: char) -> Result<(), i32> {
        self.writes += 1;
        if self.writes == self.reject_write {
            return Err(-71);
        }
        let mut bytes = [0; 4];
        let text = ch.encode_utf8(&mut bytes);
        if name.len + text.len() > name.bytes.len() {
            return Err(-72);
        }
        name.bytes[name.len..name.len + text.len()].copy_from_slice(text.as_bytes());
        name.len += text.len();
        Ok(())
    }
    fn finish_name(&mut self, name: StackName, kind: FontFamilyKind) -> Result<(), i32> {
        if self.len == self.names.len() {
            return Err(-73);
        }
        self.names[self.len] = Some((kind, name));
        self.len += 1;
        Ok(())
    }
}

#[test]
fn sink_preserves_all_family_kinds_quotes_escapes_and_trivia() {
    for source in [
        r#"SeRiF, SANS-SERIF, Cursive, fantasy, monospace, "SERIF", '\41 rial'"#,
        r"\53 ERIF, \sans-serif, Noto/**/Sans, ACME\,Inc",
        r#""\0 \D800 \110000", "A\"B\\C\,D", "", ,"#,
        "\u{80}Face, Ελληνικά, 𐐀Name",
        "\"A\\\r\nB\", --Face, -\\31 Face, serif Extra",
    ] {
        let expected = parse_font_families(source).unwrap();
        let mut sink = StackSink::new();
        let mut bytes = 0;
        parse_font_families_into_with_control(
            source,
            &mut |work| {
                bytes += work;
                Ok(())
            },
            &mut sink,
        )
        .unwrap();
        assert_eq!(sink.logical(), expected, "{source:?}");
        assert_eq!(bytes, source.len());
    }
}

#[test]
fn old_scan_only_callback_sequence_is_unchanged() {
    let mut calls = Vec::new();
    parse_font_families_with_control("sans-serif", |work| {
        calls.push(work);
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(calls, vec![0, 10]);
    let source = format!("/*{}*/ serif", "x".repeat(9000));
    let mut calls = Vec::new();
    parse_font_families_with_control(&source, |work| {
        calls.push(work);
        Ok::<_, ()>(())
    })
    .unwrap();
    assert_eq!(calls, vec![0, 4096, 4096, source.len() - 8192]);
}

#[test]
fn sink_error_stops_before_next_name_or_callback() {
    let mut sink = StackSink::new();
    sink.reject_write = 3;
    let mut calls = 0;
    let result = parse_font_families_into_with_control(
        "\"abc\", Later",
        &mut |_| {
            calls += 1;
            Ok(())
        },
        &mut sink,
    );
    assert_eq!(result, Err(FontFamilyParseError::Control(-71)));
    assert_eq!(sink.starts, 1);
    assert_eq!(sink.writes, 3);
    assert_eq!(sink.len, 0);
    // Entry, source prepay, new name, and two successful char boundaries.
    assert_eq!(calls, 5);
}

#[test]
fn second_source_window_fails_before_constructing_any_name() {
    let source = format!("/*{}*/ serif", "x".repeat(9000));
    let mut windows = 0;
    let mut sink = StackSink::new();
    let result = parse_font_families_into_with_control(
        &source,
        &mut |work| {
            if work != 0 {
                windows += 1;
            }
            if windows == 2 {
                Err(-81)
            } else {
                Ok(())
            }
        },
        &mut sink,
    );
    assert_eq!(result, Err(FontFamilyParseError::Control(-81)));
    assert_eq!(sink.starts, 0);
    assert_eq!(windows, 2);
}

#[derive(Debug, Default)]
struct RefundState {
    next: AtomicU32,
    reject_release: AtomicBool,
    releases: AtomicU32,
}
#[derive(Debug)]
struct RefundBackend(Arc<RefundState>);
impl Backend for RefundBackend {
    fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, BudgetError> {
        Ticket::new(1, self.0.next.fetch_add(1, Ordering::SeqCst) + 1, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), BudgetError> {
        Ok(())
    }
    fn release_after_free(&self, _: Ticket) -> Result<(), BudgetError> {
        self.0.releases.fetch_add(1, Ordering::SeqCst);
        if self.0.reject_release.swap(false, Ordering::SeqCst) {
            Err(BudgetError::native(-123))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _: u64) -> Result<(), BudgetError> {
        Ok(())
    }
}
#[derive(Debug)]
struct PaidName(PaidUtf8);
impl AsRef<str> for PaidName {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
#[derive(Debug)]
struct RefundSink {
    resources: ResourcesRef,
    state: Arc<RefundState>,
    starts: usize,
    finishes: usize,
    fail_generic_cleanup: bool,
}
impl FontFamilySink<BudgetError> for RefundSink {
    type Name = PaidName;
    fn new_name(&mut self) -> Result<PaidName, BudgetError> {
        self.starts += 1;
        Ok(PaidName(PaidUtf8::new(
            self.resources.clone(),
            ResourceKind::Scratch,
        )))
    }
    fn push_char(&mut self, name: &mut PaidName, ch: char) -> Result<(), BudgetError> {
        name.0.try_push(ch)
    }
    fn finish_name(&mut self, _name: PaidName, kind: FontFamilyKind) -> Result<(), BudgetError> {
        self.finishes += 1;
        if kind != FontFamilyKind::Named && self.fail_generic_cleanup {
            self.state.reject_release.store(true, Ordering::SeqCst);
        }
        // _name retires naturally on this method's return, as when a generic
        // classification discards its transient decoded spelling.
        Ok(())
    }
}
fn refund_sink() -> RefundSink {
    let state = Arc::new(RefundState::default());
    let resources = ResourcesRef::new_prepaid(
        RefundBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    RefundSink {
        resources,
        state,
        starts: 0,
        finishes: 0,
        fail_generic_cleanup: false,
    }
}

#[test]
fn syntax_cleanup_fault_is_control_and_never_syntax_fallback() {
    let mut sink = refund_sink();
    sink.state.reject_release.store(true, Ordering::SeqCst);
    let r = sink.resources.clone();
    let result = parse_font_families_into_with_control(
        "\"abc",
        &mut |work| r.checkpoint(work as u64),
        &mut sink,
    );
    assert_eq!(
        result,
        Err(FontFamilyParseError::Control(BudgetError::native(-123)))
    );
    assert_eq!(sink.starts, 1);
    assert_eq!(sink.finishes, 0);
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(r.statistics().live_leases, 1); // Physically freed, refund failed.
}

#[test]
fn generic_name_refund_fault_stops_before_the_next_family() {
    let mut sink = refund_sink();
    sink.fail_generic_cleanup = true;
    let r = sink.resources.clone();
    let result = parse_font_families_into_with_control(
        "serif, Later",
        &mut |work| r.checkpoint(work as u64),
        &mut sink,
    );
    assert_eq!(
        result,
        Err(FontFamilyParseError::Control(BudgetError::native(-123)))
    );
    assert_eq!(sink.starts, 1);
    assert_eq!(sink.finishes, 1);
    assert_eq!(r.failure(), Some(BudgetError::native(-123)));
    assert_eq!(r.statistics().release_failures, 1);
}
