// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual request-owned CSS contract and physical System allocator observations.
#![cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use simplecss::{controlled, AttributeOperator, Element, PseudoClass};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Mutex,
    },
};
const N: usize = 8192;
static LOCK: Mutex<()> = Mutex::new(());
static NEXT: AtomicUsize = AtomicUsize::new(0);
static USED: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
static CALLS: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicU64 = AtomicU64::new(u64::MAX);
static REJECT_BYTES: AtomicUsize = AtomicUsize::new(usize::MAX);
static REJECT_RELEASE: AtomicUsize = AtomicUsize::new(0);
static SIZES: [AtomicUsize; N] = [const { AtomicUsize::new(0) }; N];
static POINTERS: [AtomicUsize; N] = [const { AtomicUsize::new(0) }; N];
static PAIRED: [AtomicUsize; N] = [const { AtomicUsize::new(0) }; N];
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static UNPAID: AtomicUsize = AtomicUsize::new(0);
static UNKNOWN_FREE: AtomicUsize = AtomicUsize::new(0);
static EARLY_REFUND: AtomicUsize = AtomicUsize::new(0);
thread_local! {static OBSERVE:Cell<bool>=const{Cell::new(false)};}
struct Observer;
// SAFETY: this test allocator forwards the original layouts and pointers to System.
// Fixed atomic records do not allocate, and observation is enabled only on this test thread.
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let observed = OBSERVE.try_with(Cell::get).unwrap_or(false);
        let ticket = if observed {
            (0..NEXT.load(Ordering::SeqCst)).find(|i| {
                SIZES[*i].load(Ordering::SeqCst) == layout.size()
                    && PAIRED[*i]
                        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
            })
        } else {
            None
        };
        let ptr = unsafe { System.alloc(layout) };
        if observed {
            ALLOCATIONS.fetch_add(1, Ordering::SeqCst);
            if let Some(i) = ticket {
                POINTERS[i].store(ptr as usize, Ordering::SeqCst);
            } else {
                UNPAID.fetch_add(1, Ordering::SeqCst);
            }
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let observed = OBSERVE.try_with(Cell::get).unwrap_or(false);
        let ticket = if observed {
            (0..NEXT.load(Ordering::SeqCst))
                .find(|i| POINTERS[*i].load(Ordering::SeqCst) == ptr as usize && ptr as usize != 0)
        } else {
            None
        };
        unsafe { System.dealloc(ptr, layout) };
        if observed {
            if let Some(i) = ticket {
                POINTERS[i].store(0, Ordering::SeqCst);
            } else {
                UNKNOWN_FREE.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // A paid CSS growth must use two fully paid allocations, never System.realloc.
        if OBSERVE.try_with(Cell::get).unwrap_or(false) {
            UNPAID.fetch_add(1, Ordering::SeqCst);
        }
        unsafe { System.realloc(ptr, old, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: Observer = Observer;
#[derive(Clone, Copy)]
struct TestBackend;
impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        if REJECT_BYTES.load(Ordering::SeqCst) == 0 || bytes == REJECT_BYTES.load(Ordering::SeqCst)
        {
            return Err(Error::native(-813));
        }
        let i = NEXT.fetch_add(1, Ordering::SeqCst);
        assert!(i < N);
        if !OBSERVE.try_with(Cell::get).unwrap_or(false) {
            PAIRED[i].store(2, Ordering::SeqCst);
        }
        SIZES[i].store(bytes, Ordering::SeqCst);
        let used = USED.fetch_add(bytes as u64, Ordering::SeqCst) + bytes as u64;
        PEAK.fetch_max(used, Ordering::SeqCst);
        Ticket::new(1, i as u32 + 1, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let i = u64::from(ticket.slot()) as usize - 1;
        if OBSERVE.try_with(Cell::get).unwrap_or(false) && POINTERS[i].load(Ordering::SeqCst) != 0 {
            EARLY_REFUND.fetch_add(1, Ordering::SeqCst);
        }
        if REJECT_RELEASE.swap(0, Ordering::SeqCst) != 0 {
            return Err(Error::native(-814));
        }
        let bytes = SIZES[i].swap(0, Ordering::SeqCst);
        assert!(bytes != 0);
        USED.fetch_sub(bytes as u64, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let c = CALLS.fetch_add(1, Ordering::SeqCst);
        if c >= CANCEL.load(Ordering::SeqCst) {
            Err(Error::new(FailureKind::Cancelled, LimitKind::Work))
        } else {
            Ok(())
        }
    }
}
fn reset() {
    // Begin a fresh request case after prior owners have left their test scope.
    assert!(POINTERS.iter().all(|p| p.load(Ordering::SeqCst) == 0));
    USED.store(0, Ordering::SeqCst);
    for i in 0..N {
        SIZES[i].store(0, Ordering::SeqCst);
        POINTERS[i].store(0, Ordering::SeqCst);
        PAIRED[i].store(0, Ordering::SeqCst);
    }
    for a in [&NEXT, &ALLOCATIONS, &UNPAID, &UNKNOWN_FREE, &EARLY_REFUND] {
        a.store(0, Ordering::SeqCst);
    }
    PEAK.store(0, Ordering::SeqCst);
    CALLS.store(0, Ordering::SeqCst);
    CANCEL.store(u64::MAX, Ordering::SeqCst);
    REJECT_BYTES.store(usize::MAX, Ordering::SeqCst);
    REJECT_RELEASE.store(0, Ordering::SeqCst);
}
fn request(limits: Limits) -> ResourcesRef {
    ResourcesRef::new_prepaid(TestBackend, FirstFailureRef::new_prepaid(), limits)
}
fn observe<T>(f: impl FnOnce() -> T) -> T {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            OBSERVE.with(|v| v.set(false));
        }
    }
    OBSERVE.with(|v| v.set(true));
    let guard = Guard;
    let result = f();
    drop(guard);
    result
}
struct Node<'a> {
    names: &'a [&'a str],
    index: usize,
}
impl Element for Node<'_> {
    fn parent_element(&self) -> Option<Self> {
        self.index.checked_sub(1).map(|index| Self {
            names: self.names,
            index,
        })
    }
    fn prev_sibling_element(&self) -> Option<Self> {
        self.parent_element()
    }
    fn has_local_name(&self, name: &str) -> bool {
        self.names[self.index] == name
    }
    fn attribute_matches(&self, name: &str, op: AttributeOperator<'_>) -> bool {
        match name {
            "class" => op.matches("a b"),
            "id" => op.matches("target"),
            _ => false,
        }
    }
    fn pseudo_class_matches(&self, class: PseudoClass<'_>) -> bool {
        matches!(class, PseudoClass::FirstChild) && self.index == 0
    }
}
#[test]
fn paid_rules_match_legacy_cascade_order_and_declarations() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let r = request(Limits::default());
    let mut paid = controlled::StyleSheet::new(r.clone());
    let mut legacy = simplecss::StyleSheet::new();
    for source in [
        "g{fill:red} .a,.b {stroke:blue!important} #target{opacity:.7}",
        "g{fill:green} .a{stroke:yellow} @media all { p{} }",
        "g{font:italic 12px Arial;marker:url(#m)} div[lang|='en']{fill:black}",
    ] {
        legacy.parse_more(source);
        paid.try_parse_more(source).unwrap();
    }
    let a: Vec<_> = legacy
        .rules
        .iter()
        .map(|x| (x.selector.specificity(), x.declarations.clone()))
        .collect();
    let b: Vec<_> = paid
        .rules()
        .map(|x| (x.specificity(), x.declarations().to_vec()))
        .collect();
    assert_eq!(a, b);
    let names = ["svg", "g", "g"];
    let node = Node {
        names: &names,
        index: 2,
    };
    let a: Vec<_> = legacy
        .rules
        .iter()
        .map(|x| x.selector.matches(&node))
        .collect();
    let b: Vec<_> = paid.rules().map(|x| x.matches(&node).unwrap()).collect();
    assert_eq!(a, b);
    drop(paid);
    r.validate_retired().unwrap();
}
#[test]
fn actual_allocations_are_prepaid_and_physically_freed_before_refund() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let source = format!(
        "{}{{fill:red;stroke:blue;opacity:.5}} svg g div > .a {{fill:green}}",
        (0..1024)
            .map(|i| format!(".c{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let r = request(Limits::default());
    let names = ["svg", "g", "div", "g", "div"];
    let node = Node {
        names: &names,
        index: 4,
    };
    observe(|| {
        let mut sheet = controlled::StyleSheet::new(r.clone());
        sheet.try_parse_more(&source).unwrap();
        for rule in sheet.rules() {
            let _ = rule.matches(&node).unwrap();
        }
        drop(sheet);
    });
    assert!(ALLOCATIONS.load(Ordering::SeqCst) > 10);
    assert_eq!(UNPAID.load(Ordering::SeqCst), 0);
    assert_eq!(UNKNOWN_FREE.load(Ordering::SeqCst), 0);
    assert_eq!(EARLY_REFUND.load(Ordering::SeqCst), 0);
    assert_eq!(USED.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
    eprintln!(
        "CSS_OBSERVER allocations={} peak={} reservations={}",
        ALLOCATIONS.load(Ordering::SeqCst),
        PEAK.load(Ordering::SeqCst),
        NEXT.load(Ordering::SeqCst)
    );
}
#[test]
fn long_comments_strings_and_error_positions_are_cancellable_without_utf8_panics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cases = [
        format!("{}g{{fill:red}}", "/**/".repeat(100_000)),
        format!("g{{font-family:'{}';fill:red}}", "中文🙂".repeat(30_000)),
        format!(
            "{}@bad{{{{{{}}}}}}",
            "
"
            .repeat(100_000)
        ),
        format!("g{{{}:red}}", "x".repeat(100_000)),
    ];
    for (case, source) in cases.into_iter().enumerate() {
        reset();
        let r = request(Limits::default());
        CANCEL.store([5, 3, 1, 2][case], Ordering::SeqCst);
        let mut sheet = controlled::StyleSheet::new(r.clone());
        let result = sheet.try_parse_more(&source);
        assert!(
            result.is_err(),
            "case {case}, callbacks {}",
            CALLS.load(Ordering::SeqCst)
        );
        let error = result.unwrap_err();
        assert_eq!(error.kind, FailureKind::Cancelled);
        drop(sheet);
        r.validate_retired().unwrap();
    }
    reset();
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more("g{fill:red}").unwrap();
    drop(sheet);
    r.validate_retired().unwrap();
}
#[test]
fn iterative_matcher_preserves_deep_child_adjacent_and_descendant_backtracking() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let names = vec!["g"; 10_000];
    let node = Node {
        names: &names,
        index: names.len() - 1,
    };
    for join in [" > ", " + "] {
        let source = format!("{}{{fill:red}}", vec!["g"; 10_000].join(join));
        let r = request(Limits::default());
        let mut sheet = controlled::StyleSheet::new(r.clone());
        sheet.try_parse_more(&source).unwrap();
        assert!(sheet.rules().next().unwrap().matches(&node).unwrap());
        drop(sheet);
        r.validate_retired().unwrap();
    }
    let source = "svg g g {fill:red}";
    let names = ["svg", "g", "div", "g", "div", "g"];
    let node = Node {
        names: &names,
        index: 5,
    };
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(source).unwrap();
    assert!(sheet.rules().next().unwrap().matches(&node).unwrap());
    drop(sheet);
    r.validate_retired().unwrap();
}
#[test]
fn actual_parse_checkpoint_sweep_returns_cancelled_then_retires() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let source="/*中文*/g,.a,#target{fill:red;stroke:blue}.b > g + g:first-child {opacity:.5} @skip{foo{bar}}";
    reset();
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(source).unwrap();
    let count = CALLS.load(Ordering::SeqCst);
    drop(sheet);
    r.validate_retired().unwrap();
    for offset in 0..count {
        reset();
        let r = request(Limits::default());
        CANCEL.store(offset, Ordering::SeqCst);
        let mut sheet = controlled::StyleSheet::new(r.clone());
        assert_eq!(
            sheet.try_parse_more(source).unwrap_err().kind,
            FailureKind::Cancelled
        );
        drop(sheet);
        r.validate_retired().unwrap();
    }
}
#[test]
fn inline_preserves_legacy_tokens_and_reports_cancellation() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let source = "fill:red; font:'中文🙂'; opacity:.5!important";
    let r = request(Limits::default());
    let a: Vec<_> = simplecss::DeclarationTokenizer::from(source).collect();
    let b: Vec<_> = controlled::DeclarationTokenizer::new(source, r.clone())
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(a, b);
    let source = format!("font:'{}';fill:red", "中🙂".repeat(100_000));
    CANCEL.store(CALLS.load(Ordering::SeqCst) + 3, Ordering::SeqCst);
    assert_eq!(
        controlled::DeclarationTokenizer::new(&source, r.clone())
            .next()
            .unwrap()
            .unwrap_err()
            .kind,
        FailureKind::Cancelled
    );
    r.validate_retired().unwrap();
}
#[test]
fn measured_live_peak_minus_equal_plus_one_respects_the_actual_scratch_boundary() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let source = format!(
        "{}{{fill:red;stroke:blue}}",
        (0..200)
            .map(|i| format!(".class{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    reset();
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(&source).unwrap();
    let peak = PEAK.load(Ordering::SeqCst);
    drop(sheet);
    r.validate_retired().unwrap();
    assert!(peak > 1);
    for limit in [peak - 1, peak, peak + 1] {
        reset();
        let r = request(Limits {
            scratch_bytes: limit,
            ..Limits::default()
        });
        let mut sheet = controlled::StyleSheet::new(r.clone());
        let result = sheet.try_parse_more(&source);
        if limit < peak {
            let error = result.unwrap_err();
            assert_eq!(error.kind, FailureKind::Quota);
            assert_eq!(error.limit, LimitKind::Scratch);
        } else {
            result.unwrap();
        }
        drop(sheet);
        r.validate_retired().unwrap();
        assert_eq!(USED.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn paid_backtracking_frames_cancel_and_physically_retire_without_cloning_elements() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let source = format!("missing {}{{fill:red}}", vec!["g"; 20].join(" "));
    let names = vec!["g"; 200];
    let node = Node {
        names: &names,
        index: names.len() - 1,
    };
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(&source).unwrap();
    let before = NEXT.load(Ordering::SeqCst);
    CANCEL.store(CALLS.load(Ordering::SeqCst) + 5, Ordering::SeqCst);
    let error = observe(|| sheet.rules().next().unwrap().matches(&node)).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert!(NEXT.load(Ordering::SeqCst) > before);
    assert!(ALLOCATIONS.load(Ordering::SeqCst) > 0);
    assert_eq!(UNPAID.load(Ordering::SeqCst), 0);
    assert_eq!(UNKNOWN_FREE.load(Ordering::SeqCst), 0);
    assert_eq!(EARLY_REFUND.load(Ordering::SeqCst), 0);
    drop(sheet);
    r.validate_retired().unwrap();
    assert_eq!(USED.load(Ordering::SeqCst), 0);
}
#[test]
fn quota_and_original_native_failures_survive_recovery_and_cleanup() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let r = request(Limits {
        scratch_bytes: 1,
        ..Limits::default()
    });
    let mut sheet = controlled::StyleSheet::new(r.clone());
    assert_eq!(
        sheet.try_parse_more("g{fill:red}").unwrap_err().kind,
        FailureKind::Quota
    );
    drop(sheet);
    r.validate_retired().unwrap();
    reset();
    let r = request(Limits::default());
    REJECT_BYTES.store(0, Ordering::SeqCst);
    let mut sheet = controlled::StyleSheet::new(r.clone());
    assert_eq!(
        sheet.try_parse_more("g{fill:red}").unwrap_err(),
        Error::native(-813)
    );
    drop(sheet);
    r.validate_retired().unwrap();
    reset();
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more("g{fill:red}").unwrap();
    REJECT_RELEASE.store(1, Ordering::SeqCst);
    drop(sheet);
    assert_eq!(r.failure(), Some(Error::native(-814)));
    assert!(r.validate_retired().is_err());
    assert_eq!(r.statistics().release_failures, 1);
    assert!(USED.load(Ordering::SeqCst) > 0);
    // The last negative deliberately retains a failed lease/charge. Do not clear
    // either ledger or backend accounting or claim full retirement for this case.
}

#[test]
fn invalid_unicode_trim_checkpoints_keep_resource_errors_fatal() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for source in [
        format!("g{{font-family:{}}}", "\u{2003}".repeat(50_000)),
        format!("g:lang({}){{fill:red}}", "\u{2003}".repeat(50_000)),
    ] {
        reset();
        let r = request(Limits::default());
        let mut sheet = controlled::StyleSheet::new(r.clone());
        sheet.try_parse_more(&source).unwrap();
        let calls = CALLS.load(Ordering::SeqCst);
        drop(sheet);
        r.validate_retired().unwrap();
        assert!(calls >= 3);
        for cancel in 0..calls {
            reset();
            let r = request(Limits::default());
            CANCEL.store(cancel, Ordering::SeqCst);
            let mut sheet = controlled::StyleSheet::new(r.clone());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sheet.try_parse_more(&source)
            }));
            assert_eq!(
                result
                    .expect("UTF8 trim cancellation must not panic")
                    .unwrap_err()
                    .kind,
                FailureKind::Cancelled
            );
            drop(sheet);
            r.validate_retired().unwrap();
        }
    }
}

#[test]
fn discarded_selectors_and_empty_groups_reuse_paid_arena_slots() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let source = format!(
        "{}{}g{{fill:blue}}",
        ".a:unsupported{fill:red}".repeat(10_000),
        "g .a {}".repeat(10_000)
    );
    let r = request(Limits {
        scratch_bytes: 16 * 1024,
        ..Limits::default()
    });
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(&source).unwrap();
    assert_eq!(sheet.rules().count(), 1);
    assert_eq!(
        sheet.rules().next().unwrap().declarations()[0].value,
        "blue"
    );
    drop(sheet);
    r.validate_retired().unwrap();
    assert_eq!(USED.load(Ordering::SeqCst), 0);
}

#[test]
fn long_numeric_and_hex_declarations_scale_linearly_without_more_work_allowance() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for value in ["red", "1", "0.5", "#123", "#abcdef"] {
        let mut work = [0; 3];
        for (slot, n) in [1000, 2000, 10000].into_iter().enumerate() {
            reset();
            let source = format!("g{{{}}}", format!("opacity:{value};").repeat(n));
            let r = request(Limits::default());
            let mut sheet = controlled::StyleSheet::new(r.clone());
            sheet.try_parse_more(&source).unwrap();
            assert_eq!(sheet.rules().next().unwrap().declarations().len(), n);
            assert!(sheet
                .rules()
                .next()
                .unwrap()
                .declarations()
                .iter()
                .all(|d| d.value == value));
            work[slot] = r.statistics().work;
            println!(
                "CSS_LINEAR value={value} n={n} bytes={} work={}",
                source.len(),
                work[slot]
            );
            drop(sheet);
            r.validate_retired().unwrap();
        }
        assert!(work[1] < work[0] * 3, "doubling {value}: {work:?}");
        assert!(work[2] < work[1] * 8, "fivefold {value}: {work:?}");
    }
}

#[test]
fn font_extent_controlled_raw_slices_match_plain_without_allocating() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for source in [
        r#"font-family:"\53 ERIF", \73 erif, ACME\ Sans;fill:red"#,
        r#"font:italic 12px "A; } !", ACME\,Inc!important;fill:red"#,
        r#"font-family:"A\\", "A\\\"B", A\!B/* ; } !important */;fill:red"#,
        "font-family:\"A\\\r\nB\", \u{80}Face, Face\u{2003};fill:red",
        "font-family:First, \"unfinished; } !important;fill:red",
        "font-family:First/* unfinished; } !important;fill:red  ",
    ] {
        reset();
        let r = request(Limits::default());
        let plain: Vec<_> = simplecss::DeclarationTokenizer::from(source).collect();
        let paid: Vec<_> = controlled::DeclarationTokenizer::new(source, r.clone())
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(paid, plain);
        for declaration in paid {
            let offset = declaration.value.as_ptr() as usize - source.as_ptr() as usize;
            assert!(offset + declaration.value.len() <= source.len());
        }
        let mut tokenizer = controlled::DeclarationTokenizer::new(source, r.clone());
        assert_eq!(observe(|| tokenizer.next().unwrap().unwrap()), plain[0]);
        assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
        assert_eq!(UNPAID.load(Ordering::SeqCst), 0);
        drop(tokenizer);
        r.validate_retired().unwrap();
    }
}

#[test]
fn font_extent_long_raw_scans_cancel_without_a_successful_prefix() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for value in [
        format!("First/*{} ; }} !important */", "中🙂".repeat(6000)),
        format!("\"{}\"", "中🙂".repeat(6000)),
        format!("A{}", "\\41 ".repeat(10000)),
        format!("First, \"{}; }} !important;fill:red", "中🙂".repeat(6000)),
    ] {
        let source = format!("font-family:{value}");
        reset();
        let r = request(Limits::default());
        let first = controlled::DeclarationTokenizer::new(&source, r.clone())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(first.value, value);
        let calls = CALLS.load(Ordering::SeqCst);
        assert!(calls >= 10);
        r.validate_retired().unwrap();
        for cancel in 0..calls {
            reset();
            let r = request(Limits::default());
            CANCEL.store(cancel, Ordering::SeqCst);
            let mut tokenizer = controlled::DeclarationTokenizer::new(&source, r.clone());
            let error = tokenizer.next().unwrap().unwrap_err();
            assert_eq!(error, Error::new(FailureKind::Cancelled, LimitKind::Work));
            assert_eq!(r.failure(), Some(error));
            assert!(tokenizer.next().is_none());
            assert_eq!(USED.load(Ordering::SeqCst), 0);
            drop(tokenizer);
            r.validate_retired().unwrap();
        }
    }
}

#[test]
fn font_extent_work_and_stylesheet_failures_preserve_first_error_and_retire() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let value = format!("\"{}\", ACME\\,Inc", "中🙂".repeat(12000));
    let inline = format!("font-family:{value};fill:red");
    reset();
    let r = request(Limits {
        work: 65_792,
        ..Limits::default()
    });
    let error = controlled::DeclarationTokenizer::new(&inline, r.clone())
        .next()
        .unwrap()
        .unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Work));
    assert_eq!(r.failure(), Some(error));
    r.validate_retired().unwrap();

    let source = format!("g{{font:12px {value};fill:red}}");
    reset();
    let r = request(Limits::default());
    let mut sheet = controlled::StyleSheet::new(r.clone());
    sheet.try_parse_more(&source).unwrap();
    let calls = CALLS.load(Ordering::SeqCst);
    let declaration = sheet.rules().next().unwrap().declarations()[0];
    assert_eq!(declaration.value, format!("12px {value}"));
    drop(sheet);
    r.validate_retired().unwrap();
    for cancel in 0..calls {
        reset();
        let r = request(Limits::default());
        CANCEL.store(cancel, Ordering::SeqCst);
        let mut sheet = controlled::StyleSheet::new(r.clone());
        let error = sheet.try_parse_more(&source).unwrap_err();
        assert_eq!(error, Error::new(FailureKind::Cancelled, LimitKind::Work));
        assert_eq!(r.failure(), Some(error));
        drop(sheet);
        assert_eq!(USED.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }

    reset();
    let r = request(Limits::default());
    let first = Error::native(-1972);
    r.record(first);
    let error = controlled::DeclarationTokenizer::new(&inline, r.clone())
        .next()
        .unwrap()
        .unwrap_err();
    assert_eq!(error, first);
    assert_eq!(r.failure(), Some(first));
    r.validate_retired().unwrap();
}
