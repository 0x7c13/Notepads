// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual RequestWriter consumers. Host bookkeeping is test bootstrap, not a
//! product allocation/RSS claim. The original tests.rs inputs stay unchanged.
use markdown_svg_budget::{
    Backend, BudgetVec, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::{
    cell::Cell,
    collections::BTreeMap,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use xmlwriter::{Indent, Options, RequestWriter, XmlWriter};

const RESERVE: Error = Error::native(-701);
const COMMIT: Error = Error::native(-702);
const REFUND: Error = Error::native(-703);
const CHECK: Error = Error::native(-704);
const OPERATION: Error = Error::native(-705);

#[derive(Clone, Copy, Debug)]
enum Event {
    Reserve { slot: u32, bytes: usize },
    Commit { slot: u32, bytes: usize },
    Release { slot: u32, bytes: usize },
    Work(u64),
}
#[derive(Clone, Copy, Debug)]
struct LedgerEntry {
    bytes: usize,
    requested_at: usize,
    physical: Option<physical::Allocation>,
}
#[derive(Default)]
struct Host {
    next: AtomicU64,
    ledger: Mutex<BTreeMap<u32, LedgerEntry>>,
    events: Mutex<Vec<Event>>,
    reserves: AtomicU64,
    commits: AtomicU64,
    releases: AtomicU64,
    checks: AtomicU64,
    fail_reserve: AtomicU64,
    fail_commit: AtomicU64,
    fail_release: AtomicU64,
    fail_check: AtomicU64,
    work_value: AtomicU64,
    work_count: AtomicU64,
    fail_work: AtomicU64,
    failed_refund_bytes: AtomicU64,
    observe_system: AtomicBool,
    physical_commits: AtomicU64,
    physical_releases: AtomicU64,
}
struct HostBackend(Arc<Host>);
impl Backend for HostBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        assert_eq!(kind, ResourceKind::Scratch);
        let n = self.0.reserves.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_reserve.load(Ordering::SeqCst) {
            return Err(RESERVE);
        }
        let slot = self.0.next.fetch_add(1, Ordering::SeqCst) as u32 + 1;
        assert!(self
            .0
            .ledger
            .lock()
            .unwrap()
            .insert(
                slot,
                LedgerEntry {
                    bytes,
                    requested_at: 0,
                    physical: None
                }
            )
            .is_none());
        self.0
            .events
            .lock()
            .unwrap()
            .push(Event::Reserve { slot, bytes });
        // Host bookkeeping above is outside the candidate allocation interval.
        if self.0.observe_system.load(Ordering::SeqCst) {
            self.0
                .ledger
                .lock()
                .unwrap()
                .get_mut(&slot)
                .unwrap()
                .requested_at = physical::position();
        }
        Ticket::new(1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let slot = ticket.slot();
        let entry = self.0.ledger.lock().unwrap()[&slot];
        assert_eq!(entry.bytes, bytes);
        if self.0.observe_system.load(Ordering::SeqCst) {
            let allocation = physical::find_allocation(entry.requested_at, bytes);
            self.0
                .ledger
                .lock()
                .unwrap()
                .get_mut(&slot)
                .unwrap()
                .physical = Some(allocation);
            self.0.physical_commits.fetch_add(1, Ordering::SeqCst);
        }
        self.0
            .events
            .lock()
            .unwrap()
            .push(Event::Commit { slot, bytes });
        let n = self.0.commits.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_commit.load(Ordering::SeqCst) {
            Err(COMMIT)
        } else {
            Ok(())
        }
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let refund_entry_position = physical::position();
        let slot = ticket.slot();
        let entry = self.0.ledger.lock().unwrap().remove(&slot).unwrap();
        let bytes = entry.bytes;
        if self.0.observe_system.load(Ordering::SeqCst) {
            if let Some(allocation) = entry.physical {
                // The observer logs only after System.dealloc has returned.
                // This check runs at entry to the actual refund callback.
                physical::assert_freed_before(allocation, refund_entry_position);
                self.0.physical_releases.fetch_add(1, Ordering::SeqCst);
            }
        }
        self.0
            .events
            .lock()
            .unwrap()
            .push(Event::Release { slot, bytes });
        let n = self.0.releases.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_release.load(Ordering::SeqCst) {
            self.0
                .failed_refund_bytes
                .store(bytes as u64, Ordering::SeqCst);
            Err(REFUND)
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.events.lock().unwrap().push(Event::Work(work));
        let n = self.0.checks.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_check.load(Ordering::SeqCst) {
            return Err(CHECK);
        }
        if work == self.0.work_value.load(Ordering::SeqCst) {
            let m = self.0.work_count.fetch_add(1, Ordering::SeqCst) + 1;
            if m == self.0.fail_work.load(Ordering::SeqCst) {
                return Err(CHECK);
            }
        }
        Ok(())
    }
}
fn request() -> (ResourcesRef, Arc<Host>) {
    let host = Arc::new(Host::default());
    (
        ResourcesRef::new_prepaid(
            HostBackend(host.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        host,
    )
}
fn retired(resources: &ResourcesRef, host: &Host, refund_failed: bool) {
    assert!(host.ledger.lock().unwrap().is_empty());
    assert_eq!(
        resources.statistics().release_failures,
        u64::from(refund_failed)
    );
    assert_eq!(resources.statistics().live_leases, u64::from(refund_failed));
    if refund_failed {
        assert_eq!(
            resources.statistics().scratch_bytes,
            host.failed_refund_bytes.load(Ordering::SeqCst)
        );
        assert!(resources.validate_retired().is_err());
    } else {
        assert_eq!(resources.statistics().scratch_bytes, 0);
        resources.validate_retired().unwrap();
    }
}

struct Counter<'a>(&'a Cell<usize>);
impl fmt::Display for Counter<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.set(self.0.get() + 1);
        out.write_str("counter")
    }
}
struct LongFormat<'a> {
    text: &'a str,
    entered: &'a Cell<usize>,
    after_write: &'a Cell<usize>,
}
impl fmt::Display for LongFormat<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.entered.set(self.entered.get() + 1);
        out.write_str(self.text)?;
        self.after_write.set(self.after_write.get() + 1);
        Ok(())
    }
}

#[test]
fn second_format_output_window_stops_formatter_and_next_argument() {
    let (resources, host) = request();
    let mut writer = RequestWriter::try_new(Options::default(), &resources).unwrap();
    // Retain enough already paid capacity so the operation below cannot hide
    // its missing output checks behind BudgetVec growth-copy checkpoints.
    writer.try_write_comment(&"p".repeat(2048)).unwrap();
    let entered = Cell::new(0);
    let after_write = Cell::new(0);
    let following = Cell::new(0);
    let text = "x".repeat(1024);
    host.work_value.store(256, Ordering::SeqCst);
    host.work_count.store(0, Ordering::SeqCst);
    host.fail_work.store(2, Ordering::SeqCst);
    let allocations_before = host.reserves.load(Ordering::SeqCst);
    let result = writer.try_write_comment_fmt(format_args!(
        "{}{}",
        LongFormat {
            text: &text,
            entered: &entered,
            after_write: &after_write
        },
        Counter(&following)
    ));
    println!("FORMAT_WINDOW result={result:?} entered={} afterWrite={} following={} matched256={} declaredWork={} newAllocations={}", entered.get(), after_write.get(), following.get(), host.work_count.load(Ordering::SeqCst), resources.statistics().work, host.reserves.load(Ordering::SeqCst)-allocations_before);
    assert_eq!(result, Err(CHECK));
    assert_eq!(host.reserves.load(Ordering::SeqCst), allocations_before);
    assert_eq!(entered.get(), 1);
    assert_eq!(after_write.get(), 0);
    assert_eq!(following.get(), 0);
    assert_eq!(resources.failure(), Some(CHECK));
    let before = host.checks.load(Ordering::SeqCst);
    assert_eq!(
        writer.try_write_comment_fmt(format_args!("{}", Counter(&following))),
        Err(CHECK)
    );
    assert_eq!(following.get(), 0);
    assert_eq!(host.checks.load(Ordering::SeqCst), before);
    drop(writer);
    retired(&resources, &host, false);
}

// The original source is included verbatim. Its local xmlwriter name resolves
// to this paid adapter, so all39 existing inputs/expected strings and their
// original panic annotations run unchanged. The unwraps are test glue only;
// separate tests below inspect RequestWriter's typed invalid-state results.
pub struct UpstreamAdapter {
    writer: Option<RequestWriter>,
    resources: ResourcesRef,
    host: Arc<Host>,
}
impl UpstreamAdapter {
    fn new(options: Options) -> Self {
        let (resources, host) = request();
        let writer = RequestWriter::try_new(options, &resources).unwrap();
        Self {
            writer: Some(writer),
            resources,
            host,
        }
    }
    fn writer(&mut self) -> &mut RequestWriter {
        self.writer.as_mut().unwrap()
    }
    fn start_element(&mut self, name: &str) {
        self.writer().try_start_element(name).unwrap();
    }
    fn end_element(&mut self) {
        self.writer().try_end_element().unwrap();
    }
    fn write_declaration(&mut self) {
        self.writer().try_write_declaration().unwrap();
    }
    fn write_attribute<V: fmt::Display + ?Sized>(&mut self, name: &str, value: &V) {
        self.writer().try_write_attribute(name, value).unwrap();
    }
    fn write_comment(&mut self, text: &str) {
        self.writer().try_write_comment(text).unwrap();
    }
    fn write_text(&mut self, text: &str) {
        self.writer().try_write_text(text).unwrap();
    }
    fn set_preserve_whitespaces(&mut self, value: bool) {
        self.writer().try_set_preserve_whitespaces(value).unwrap();
    }
    fn end_document(mut self) -> String {
        String::from_utf8(self.writer().try_finish().unwrap().to_vec()).unwrap()
    }
}
impl Drop for UpstreamAdapter {
    fn drop(&mut self) {
        drop(self.writer.take());
        retired(&self.resources, &self.host, false);
    }
}
mod funded_original {
    mod xmlwriter {
        pub(super) use super::super::UpstreamAdapter as XmlWriter;
        pub(super) use ::xmlwriter::{Indent, Options};
    }
    include!("tests.rs");
    #[test]
    fn original_source_really_resolves_to_paid_adapter() {
        let _: super::UpstreamAdapter = XmlWriter::new(Options::default());
    }
}

fn fixture_paid(writer: &mut RequestWriter, name: &str, payload: &str) -> Result<(), Error> {
    writer.try_write_declaration()?;
    writer.try_start_element(name)?;
    writer.try_write_attribute("quotes", payload)?;
    writer.try_write_attribute_fmt(
        "float",
        format_args!(
            "{:08.3} {} {:e} {:?}",
            1.234567_f64,
            -0.0_f32,
            1.0e-37_f32,
            f64::NAN
        ),
    )?;
    for i in 0..37 {
        writer.try_start_element("child")?;
        writer.try_write_attribute("index", &i)?;
        if i % 7 == 0 {
            writer.try_write_comment("注释 ' \" <&> 🙂")?;
        }
    }
    writer.try_set_preserve_whitespaces(true)?;
    writer.try_write_text("Unicode 中文 é🙂 < & > &lt;\n  whitespace")?;
    writer.try_end_element()?;
    writer.try_set_preserve_whitespaces(false)?;
    Ok(())
}
fn fixture_plain(options: Options, name: &str, payload: &str) -> String {
    let mut writer = XmlWriter::new(options);
    writer.write_declaration();
    writer.start_element(name);
    writer.write_attribute("quotes", payload);
    writer.write_attribute_fmt(
        "float",
        format_args!(
            "{:08.3} {} {:e} {:?}",
            1.234567_f64,
            -0.0_f32,
            1.0e-37_f32,
            f64::NAN
        ),
    );
    for i in 0..37 {
        writer.start_element("child");
        writer.write_attribute("index", &i);
        if i % 7 == 0 {
            writer.write_comment("注释 ' \" <&> 🙂");
        }
    }
    writer.set_preserve_whitespaces(true);
    writer.write_text("Unicode 中文 é🙂 < & > &lt;\n  whitespace");
    writer.end_element();
    writer.set_preserve_whitespaces(false);
    writer.end_document()
}
fn fixture_run(
    resources: &ResourcesRef,
    expected: &[u8],
    name: &str,
    payload: &str,
    options: Options,
) -> Result<usize, Error> {
    let mut writer = RequestWriter::try_new(options, resources)?;
    let candidate = (|| {
        fixture_paid(&mut writer, name, payload)?;
        let bytes = writer.try_finish()?;
        assert_eq!(bytes, expected);
        Ok(bytes.len())
    })();
    let candidate = candidate.map_err(|error| resources.record(error));
    drop(writer); // Both real byte/depth owners retire before publication.
    resources.checkpoint(0)?;
    candidate
}

#[test]
fn unicode_quotes_float_indent_preserve_long_closing_name_and_depth_match_all_bytes() {
    let name = format!("根{}终", "节点🙂".repeat(137));
    let payload = "'\"<&> é🙂 ".repeat(23);
    for (case, options) in [
        Options::default(),
        Options {
            use_single_quote: true,
            ..Options::default()
        },
        Options {
            indent: Indent::None,
            attributes_indent: Indent::None,
            use_single_quote: false,
        },
        Options {
            indent: Indent::Tabs,
            attributes_indent: Indent::Tabs,
            use_single_quote: true,
        },
        Options {
            indent: Indent::Spaces(1),
            attributes_indent: Indent::Spaces(2),
            use_single_quote: false,
        },
    ]
    .iter()
    .copied()
    .enumerate()
    {
        let expected = fixture_plain(options, &name, &payload);
        let (resources, host) = request();
        let mut writer = RequestWriter::try_new(options, &resources).unwrap();
        fixture_paid(&mut writer, &name, &payload).unwrap();
        let bytes = writer.try_finish().unwrap();
        assert_eq!(bytes, expected.as_bytes());
        if let Some(directory) = std::env::var_os("NOTEPADS_MARKDOWN_XML_TEST_DUMP") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::write(
                directory.join(format!("case-{case}-plain.xml")),
                expected.as_bytes(),
            )
            .unwrap();
            std::fs::write(directory.join(format!("case-{case}-some.xml")), bytes).unwrap();
        }
        drop(writer);
        resources.checkpoint(0).unwrap();
        assert!(host.reserves.load(Ordering::SeqCst) > 2);
        retired(&resources, &host, false);
    }
}

#[test]
fn every_real_buffer_reserve_commit_and_natural_refund_fault_keeps_first_error() {
    let name = "node".repeat(151);
    let payload = "'\"<&>é🙂".repeat(17);
    let options = Options {
        indent: Indent::None,
        ..Options::default()
    };
    let expected = fixture_plain(options, &name, &payload);
    let (r, h) = request();
    fixture_run(&r, expected.as_bytes(), &name, &payload, options).unwrap();
    let reserves = h.reserves.load(Ordering::SeqCst);
    let commits = h.commits.load(Ordering::SeqCst);
    let releases = h.releases.load(Ordering::SeqCst);
    assert_eq!(reserves, commits);
    assert_eq!(reserves, releases);
    assert!(reserves >= 10);
    let mut committed = BTreeMap::new();
    let mut refunded = BTreeMap::new();
    for event in h.events.lock().unwrap().iter() {
        match *event {
            Event::Reserve { slot, bytes } => {
                assert!(committed.insert(slot, (bytes, false)).is_none());
            }
            Event::Commit { slot, bytes } => {
                assert_eq!(committed[&slot], (bytes, false));
                committed.get_mut(&slot).unwrap().1 = true;
            }
            Event::Release { slot, bytes } => {
                assert_eq!(committed[&slot], (bytes, true));
                assert!(refunded.insert(slot, bytes).is_none());
            }
            Event::Work(_) => {}
        }
    }
    assert_eq!(refunded.len() as u64, reserves);
    println!(
        "XML_REAL_OWNERS reserve={reserves} commit={commits} release={releases} events={:?}",
        h.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, Event::Work(_)))
            .collect::<Vec<_>>()
    );
    retired(&r, &h, false);
    for mode in 0..3 {
        for fail in 1..=reserves {
            let (r, h) = request();
            let expected_error = match mode {
                0 => {
                    h.fail_reserve.store(fail, Ordering::SeqCst);
                    RESERVE
                }
                1 => {
                    h.fail_commit.store(fail, Ordering::SeqCst);
                    COMMIT
                }
                _ => {
                    h.fail_release.store(fail, Ordering::SeqCst);
                    REFUND
                }
            };
            assert_eq!(
                fixture_run(&r, expected.as_bytes(), &name, &payload, options),
                Err(expected_error),
                "mode={mode} fault={fail}"
            );
            assert_eq!(r.failure(), Some(expected_error));
            let before = (
                h.reserves.load(Ordering::SeqCst),
                h.checks.load(Ordering::SeqCst),
            );
            assert!(
                matches!(RequestWriter::try_new(Options::default(), &r), Err(e) if e == expected_error)
            );
            assert_eq!(
                (
                    h.reserves.load(Ordering::SeqCst),
                    h.checks.load(Ordering::SeqCst)
                ),
                before
            );
            retired(&r, &h, mode == 2);
        }
    }
}

struct ArmAfterFormat<'a> {
    text: &'a str,
    host: &'a Host,
    matching_window: u64,
    start: &'a Cell<usize>,
}
impl fmt::Display for ArmAfterFormat<'_> {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.write_str(self.text)?;
        self.start.set(self.host.events.lock().unwrap().len());
        self.host.work_value.store(256, Ordering::SeqCst);
        self.host.work_count.store(0, Ordering::SeqCst);
        self.host
            .fail_work
            .store(self.matching_window, Ordering::SeqCst);
        Ok(())
    }
}
fn warmed_attribute(resources: &ResourcesRef) -> RequestWriter {
    let mut writer = RequestWriter::try_new(Options::default(), resources).unwrap();
    writer.try_write_comment(&"p".repeat(2048)).unwrap();
    writer.try_start_element("r").unwrap();
    writer
}
#[test]
fn second_escape_scan_and_overlapping_shift_windows_fail_at_distinct_real_actions() {
    for shift in [false, true] {
        let (r, h) = request();
        let mut writer = warmed_attribute(&r);
        let text = if shift {
            format!("\"{}", "x".repeat(1023))
        } else {
            "x".repeat(1024)
        };
        let start = Cell::new(0);
        let window = if shift { 3 } else { 2 };
        let result = writer.try_write_attribute_fmt(
            "a",
            format_args!(
                "{}",
                ArmAfterFormat {
                    text: &text,
                    host: &h,
                    matching_window: window,
                    start: &start
                }
            ),
        );
        assert_eq!(result, Err(CHECK));
        assert_eq!(r.failure(), Some(CHECK));
        let work: Vec<_> = h.events.lock().unwrap()[start.get()..]
            .iter()
            .filter_map(|event| match event {
                Event::Work(n) if *n != 0 => Some(*n),
                _ => None,
            })
            .collect();
        assert_eq!(
            work,
            if shift {
                vec![256, 5, 256, 256]
            } else {
                vec![256, 256]
            },
            "shift={shift}"
        );
        let calls = Cell::new(0);
        assert_eq!(
            writer.try_write_attribute_fmt("later", format_args!("{}", Counter(&calls))),
            Err(CHECK)
        );
        assert_eq!(calls.get(), 0);
        drop(writer);
        retired(&r, &h, false);
    }
}

#[test]
fn finish_borrows_same_bytes_is_idempotent_retains_both_fees_and_rejects_mutation() {
    let (r, h) = request();
    let mut writer = RequestWriter::try_new(Options::default(), &r).unwrap();
    writer.try_start_element("r").unwrap();
    writer.try_write_text("中文🙂 <").unwrap();
    let (pointer, length) = {
        let bytes = writer.try_finish().unwrap();
        assert_eq!(bytes, "<r>\n    中文🙂 &lt;\n</r>\n".as_bytes());
        (bytes.as_ptr(), bytes.len())
    };
    let allocations = h.reserves.load(Ordering::SeqCst);
    let live = r.statistics().scratch_bytes;
    assert_eq!(r.statistics().live_leases, 2);
    assert_eq!(h.ledger.lock().unwrap().len(), 2);
    assert!(live > length as u64);
    let again = writer.try_finish().unwrap();
    assert_eq!(again.as_ptr(), pointer);
    assert_eq!(again.len(), length);
    assert_eq!(h.reserves.load(Ordering::SeqCst), allocations);
    let counter = Cell::new(0);
    let error = writer
        .try_write_comment_fmt(format_args!("{}", Counter(&counter)))
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Invariant);
    assert_eq!(counter.get(), 0);
    assert_eq!(writer.try_finish(), Err(error));
    drop(writer);
    retired(&r, &h, false);
}

#[test]
fn request_wrong_state_returns_typed_error_without_panicking() {
    for case in 0..6 {
        let (r, h) = request();
        let mut w = RequestWriter::try_new(Options::default(), &r).unwrap();
        let counter = Cell::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match case {
            0 => w.try_write_attribute_fmt("a", format_args!("{}", Counter(&counter))),
            1 => w.try_write_text_fmt(format_args!("{}", Counter(&counter))),
            2 => {
                w.try_write_declaration().unwrap();
                w.try_write_declaration()
            }
            3 => {
                w.try_write_comment("c").unwrap();
                w.try_write_declaration()
            }
            4 => {
                w.try_start_element("r").unwrap();
                w.try_end_element().unwrap();
                w.try_write_text_fmt(format_args!("{}", Counter(&counter)))
            }
            _ => {
                w.try_start_element("r").unwrap();
                w.try_write_text("t").unwrap();
                w.try_write_attribute_fmt("a", format_args!("{}", Counter(&counter)))
            }
        }));
        assert!(result.is_ok());
        let error = result.unwrap().unwrap_err();
        assert_eq!(error.kind, FailureKind::Invariant);
        assert_eq!(error.limit, LimitKind::None);
        assert_eq!(counter.get(), 0);
        assert_eq!(r.failure(), Some(error));
        drop(w);
        retired(&r, &h, false);
    }
    let (r, h) = request();
    let mut w = RequestWriter::try_new(Options::default(), &r).unwrap();
    w.try_end_element().unwrap();
    w.try_start_element("r").unwrap();
    assert_eq!(w.try_finish().unwrap(), b"\n<r/>\n");
    drop(w);
    retired(&r, &h, false);
}

struct FormatError;
impl fmt::Display for FormatError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        Err(fmt::Error)
    }
}
struct OwnedOperationError<'a> {
    resources: &'a ResourcesRef,
}
impl fmt::Display for OwnedOperationError<'_> {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut held = BudgetVec::new(self.resources.clone(), ResourceKind::Scratch);
        held.try_resize_copy(13, 1u8).map_err(|_| fmt::Error)?;
        self.resources.record(OPERATION); // Record before the formatter's own paid local exits.
        drop(held);
        Err(fmt::Error)
    }
}
#[test]
fn format_and_syntax_operation_errors_are_canonical_before_natural_cleanup_failure() {
    for case in 0..3 {
        let (r, h) = request();
        let mut w = RequestWriter::try_new(Options::default(), &r).unwrap();
        w.try_start_element("r").unwrap();
        // Retain spare paid capacity before arming cleanup refusal. A normal
        // growth refund must not precede the operation error tested below.
        w.try_write_text(&"x".repeat(256)).unwrap();
        w.try_write_text("warm").unwrap();
        h.fail_release
            .store(h.releases.load(Ordering::SeqCst) + 1, Ordering::SeqCst);
        let later = Cell::new(0);
        let error = match case {
            0 => w
                .try_write_text_fmt(format_args!("{}{}", FormatError, Counter(&later)))
                .unwrap_err(),
            1 => w
                .try_write_attribute_fmt("a", format_args!("{}", Counter(&later)))
                .unwrap_err(),
            _ => w
                .try_write_text_fmt(format_args!(
                    "{}{}",
                    OwnedOperationError { resources: &r },
                    Counter(&later)
                ))
                .unwrap_err(),
        };
        if case == 2 {
            assert_eq!(error, OPERATION);
        } else {
            assert_eq!(error.kind, FailureKind::Invariant);
        }
        assert_eq!(later.get(), 0);
        assert_eq!(r.failure(), Some(error));
        drop(w);
        assert_eq!(r.failure(), Some(error));
        retired(&r, &h, true);
    }
}

#[test]
fn successful_finish_followed_by_drop_refusal_is_not_a_publishable_success() {
    for owner in 1..=2 {
        let (r, h) = request();
        let mut w = RequestWriter::try_new(Options::default(), &r).unwrap();
        w.try_start_element("r").unwrap();
        w.try_write_text("t").unwrap();
        w.try_finish().unwrap();
        assert!(r.failure().is_none());
        h.fail_release
            .store(h.releases.load(Ordering::SeqCst) + owner, Ordering::SeqCst);
        drop(w);
        assert_eq!(r.failure(), Some(REFUND));
        assert_eq!(r.checkpoint(0), Err(REFUND));
        assert!(matches!(
            RequestWriter::try_new(Options::default(), &r),
            Err(REFUND)
        ));
        retired(&r, &h, true);
    }
}

#[test]
fn every_mutating_entry_rejects_finished_writer_before_formatter_or_allocation() {
    for case in 0..10 {
        let (r, h) = request();
        let mut writer = RequestWriter::try_new(Options::default(), &r).unwrap();
        writer.try_start_element("r").unwrap();
        writer.try_finish().unwrap();
        let before = (
            h.reserves.load(Ordering::SeqCst),
            r.statistics().scratch_bytes,
        );
        let counter = Cell::new(0);
        let error = match case {
            0 => writer.try_write_declaration(),
            1 => writer.try_start_element("later"),
            2 => writer.try_write_attribute("a", &Counter(&counter)),
            3 => writer.try_write_attribute_fmt("a", format_args!("{}", Counter(&counter))),
            4 => writer.try_write_comment("later"),
            5 => writer.try_write_comment_fmt(format_args!("{}", Counter(&counter))),
            6 => writer.try_set_preserve_whitespaces(true),
            7 => writer.try_write_text("later"),
            8 => writer.try_write_text_fmt(format_args!("{}", Counter(&counter))),
            _ => writer.try_end_element(),
        }
        .unwrap_err();
        assert_eq!(error, Error::new(FailureKind::Invariant, LimitKind::None));
        assert_eq!(counter.get(), 0);
        assert_eq!(
            (
                h.reserves.load(Ordering::SeqCst),
                r.statistics().scratch_bytes
            ),
            before
        );
        assert_eq!(writer.try_finish(), Err(error));
        drop(writer);
        retired(&r, &h, false);
    }
}

// Test-only allocator boundary. It delegates the real allocation and free to
// System and logs into fixed atomics, without allocating or reading freed data.
// TLS only selects this one test thread; no production policy/account uses it.
mod physical {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
        sync::{
            atomic::{AtomicBool, AtomicUsize, Ordering},
            Mutex, MutexGuard,
        },
    };
    const CAPACITY: usize = 16_384;
    struct Event {
        kind: AtomicUsize,
        pointer: AtomicUsize,
        bytes: AtomicUsize,
    }
    impl Event {
        const fn new() -> Self {
            Self {
                kind: AtomicUsize::new(0),
                pointer: AtomicUsize::new(0),
                bytes: AtomicUsize::new(0),
            }
        }
    }
    static EVENTS: [Event; CAPACITY] = [const { Event::new() }; CAPACITY];
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    static OVERFLOW: AtomicBool = AtomicBool::new(false);
    static LOCK: Mutex<()> = Mutex::new(());
    thread_local! { static CAPTURE: Cell<bool> = const { Cell::new(false) }; }
    fn record(kind: usize, pointer: *mut u8, bytes: usize) {
        if !CAPTURE.try_with(Cell::get).unwrap_or(false) {
            return;
        }
        let index = NEXT.fetch_add(1, Ordering::SeqCst);
        if index >= CAPACITY {
            OVERFLOW.store(true, Ordering::SeqCst);
            return;
        }
        EVENTS[index]
            .pointer
            .store(pointer as usize, Ordering::SeqCst);
        EVENTS[index].bytes.store(bytes, Ordering::SeqCst);
        EVENTS[index].kind.store(kind, Ordering::SeqCst);
    }
    struct ObservedSystem;
    // SAFETY: every request is passed unchanged to System. Logging only stores
    // integer addresses/layout sizes in atomics and never accesses the block.
    unsafe impl GlobalAlloc for ObservedSystem {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: forwarding GlobalAlloc's caller-provided valid layout.
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                record(1, pointer, layout.size());
            }
            pointer
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            // SAFETY: identical allocator and caller-provided layout.
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                record(1, pointer, layout.size());
            }
            pointer
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: forwarding the caller's live allocation and exact layout.
            unsafe { System.dealloc(pointer, layout) };
            record(2, pointer, layout.size()); // Strictly after the actual free.
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            // SAFETY: forwarding the same live allocation/layout/new size.
            let next = unsafe { System.realloc(pointer, layout, new_size) };
            if !next.is_null() {
                record(3, pointer, layout.size());
                record(4, next, new_size);
            }
            next
        }
    }
    #[global_allocator]
    static ALLOCATOR: ObservedSystem = ObservedSystem;
    pub(super) struct Guard {
        _lock: MutexGuard<'static, ()>,
    }
    impl Guard {
        pub(super) fn begin() -> Self {
            let lock = LOCK.lock().unwrap();
            NEXT.store(0, Ordering::SeqCst);
            OVERFLOW.store(false, Ordering::SeqCst);
            CAPTURE.with(|flag| flag.set(true));
            Self { _lock: lock }
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            CAPTURE.with(|flag| flag.set(false));
            assert!(!OVERFLOW.load(Ordering::SeqCst));
        }
    }
    #[derive(Clone, Copy, Debug)]
    pub(super) struct Allocation {
        event: usize,
        pointer: usize,
        bytes: usize,
    }
    pub(super) fn position() -> usize {
        NEXT.load(Ordering::SeqCst)
    }
    pub(super) fn find_allocation(start: usize, bytes: usize) -> Allocation {
        assert!(!OVERFLOW.load(Ordering::SeqCst));
        let mut found = None;
        for index in start..position() {
            if EVENTS[index].kind.load(Ordering::SeqCst) == 1
                && EVENTS[index].bytes.load(Ordering::SeqCst) == bytes
            {
                assert!(
                    found.is_none(),
                    "ambiguous same-size physical allocation interval"
                );
                found = Some(Allocation {
                    event: index,
                    pointer: EVENTS[index].pointer.load(Ordering::SeqCst),
                    bytes,
                });
            }
        }
        found.expect("real System allocation before this ticket's commit")
    }
    pub(super) fn assert_freed_before(allocation: Allocation, refund_entry_position: usize) {
        assert!(!OVERFLOW.load(Ordering::SeqCst));
        let mut frees = 0;
        assert!(refund_entry_position <= position());
        for index in allocation.event + 1..refund_entry_position {
            if EVENTS[index].pointer.load(Ordering::SeqCst) == allocation.pointer {
                let kind = EVENTS[index].kind.load(Ordering::SeqCst);
                if kind == 2 && EVENTS[index].bytes.load(Ordering::SeqCst) == allocation.bytes {
                    frees += 1;
                }
                // BudgetVec growth constructs a distinct new Vec; it may not
                // realloc this paid block in place or hide its deallocation.
                assert_ne!(kind, 3, "observed paid block unexpectedly reallocated");
            }
        }
        assert_eq!(
            frees, 1,
            "System.free must complete before this refund callback"
        );
    }
}

#[test]
fn system_observed_byte_and_depth_allocations_free_before_every_real_refund() {
    let name = "node".repeat(151);
    let payload = "'\"<&>é🙂".repeat(17);
    let options = Options {
        indent: Indent::None,
        ..Options::default()
    };
    let expected = fixture_plain(options, &name, &payload);
    let (r, h) = request();
    h.observe_system.store(true, Ordering::SeqCst);
    {
        let _capture = physical::Guard::begin();
        fixture_run(&r, expected.as_bytes(), &name, &payload, options).unwrap();
    }
    let total = h.reserves.load(Ordering::SeqCst);
    assert_eq!(h.physical_commits.load(Ordering::SeqCst), total);
    assert_eq!(h.physical_releases.load(Ordering::SeqCst), total);
    retired(&r, &h, false);
    println!("XML_SYSTEM positiveActualAllocations={total} positiveFreeBeforeRefund={total}");
    for mode in 0..3 {
        for failed in 1..=total {
            let (r, h) = request();
            h.observe_system.store(true, Ordering::SeqCst);
            let expected_error = match mode {
                0 => {
                    h.fail_reserve.store(failed, Ordering::SeqCst);
                    RESERVE
                }
                1 => {
                    h.fail_commit.store(failed, Ordering::SeqCst);
                    COMMIT
                }
                _ => {
                    h.fail_release.store(failed, Ordering::SeqCst);
                    REFUND
                }
            };
            {
                let _capture = physical::Guard::begin();
                assert_eq!(
                    fixture_run(&r, expected.as_bytes(), &name, &payload, options),
                    Err(expected_error)
                );
            }
            assert_eq!(
                h.physical_commits.load(Ordering::SeqCst),
                h.commits.load(Ordering::SeqCst)
            );
            assert_eq!(
                h.physical_releases.load(Ordering::SeqCst),
                h.physical_commits.load(Ordering::SeqCst)
            );
            retired(&r, &h, mode == 2);
        }
    }
}

#[test]
fn system_observer_rejects_premature_refund_while_block_is_still_live() {
    let _capture = physical::Guard::begin();
    let start = physical::position();
    let bytes = vec![17u8; 641];
    std::hint::black_box(&bytes);
    let allocation = physical::find_allocation(start, 641);
    let premature = std::panic::catch_unwind(|| {
        physical::assert_freed_before(allocation, physical::position());
    });
    assert!(
        premature.is_err(),
        "observer must reject a still-live block"
    );
    assert_eq!(std::hint::black_box(bytes[13]), 17);
    drop(bytes);
    physical::assert_freed_before(allocation, physical::position());
}
