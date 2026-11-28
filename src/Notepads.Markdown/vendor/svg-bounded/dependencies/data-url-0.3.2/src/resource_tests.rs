// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use crate::controlled::{DataUrl, Error as UrlError};
use markdown_svg_budget::{
    Backend, BudgetVec, Error, FailureKind, FirstFailureRef, LimitKind, Limits, PaidEncoded,
    ResourceKind, ResourcesRef, Ticket,
};
use std::fmt::Write;
use std::{
    eprintln, format,
    string::ToString,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    vec::Vec,
};

struct State {
    reserves: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
    callbacks: AtomicU64,
    work: AtomicU64,
    fail_at: usize,
    cancel_at: AtomicU64,
    first_reserve_callback: AtomicU64,
    late_release: AtomicBool,
    fail_release_bytes: AtomicUsize,
}
#[derive(Clone)]
struct Ledger(Arc<State>);
impl Backend for Ledger {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let n = self.0.reserves.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_at {
            return Err(Error::native(-1234567));
        }
        self.0
            .first_reserve_callback
            .compare_exchange(
                u64::MAX,
                self.0.callbacks.load(Ordering::SeqCst),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .ok();
        let live = self.0.live.fetch_add(bytes, Ordering::SeqCst) + bytes;
        self.0.peak.fetch_max(live, Ordering::SeqCst);
        let ticket = n as u64;
        observer_reserve(ticket, bytes);
        Ticket::new(bytes as u64, ticket as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        observer_release(u64::from(ticket.slot()));
        self.0
            .live
            .fetch_sub(ticket.account_nonce() as usize, Ordering::SeqCst);
        if self.0.late_release.load(Ordering::SeqCst)
            || ticket.account_nonce() as usize == self.0.fail_release_bytes.load(Ordering::SeqCst)
        {
            Err(Error::native(-777))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.work.fetch_add(work, Ordering::SeqCst);
        let n = self.0.callbacks.fetch_add(1, Ordering::SeqCst) + 1;
        if n >= self.0.cancel_at.load(Ordering::SeqCst) {
            Err(Error::new(FailureKind::Cancelled, LimitKind::Work))
        } else {
            Ok(())
        }
    }
}
fn resources(limits: Limits, fail_at: usize) -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        reserves: AtomicUsize::new(0),
        live: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        callbacks: AtomicU64::new(0),
        work: AtomicU64::new(0),
        fail_at,
        cancel_at: AtomicU64::new(u64::MAX),
        first_reserve_callback: AtomicU64::new(u64::MAX),
        late_release: AtomicBool::new(false),
        fail_release_bytes: AtomicUsize::new(usize::MAX),
    });
    let r = ResourcesRef::new_prepaid(
        Ledger(state.clone()),
        FirstFailureRef::new_prepaid(),
        limits,
    );
    (r, state)
}
fn semantic(r: &ResourcesRef, error: &UrlError) {
    if matches!(error, UrlError::InvalidBase64(_)) {
        r.record(Error::new(FailureKind::MalformedImageData, LimitKind::None));
    }
}
fn decode(source: &str, r: &ResourcesRef) -> Result<PaidEncoded, UrlError> {
    let url = DataUrl::process(source, r.clone())?;
    let length = match url.decoded_length() {
        Ok(n) => n,
        Err(e) => {
            semantic(r, &e);
            return Err(e);
        }
    };
    let _admission = r.admit_child_encoded(length)?;
    let mut output = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    output.try_reserve_exact(length)?;
    if let Err(e) = url.decode_into(length, &mut output) {
        semantic(r, &e);
        return Err(e);
    }
    drop(url);
    r.checkpoint(0)?;
    output.into_shared().map_err(UrlError::Resource)
}
// Observation forwards every allocation unchanged to System. Fixed TLS slots
// pair exact-size prior leases with actual allocations, and require physical
// free before refund. It never returns null or simulates recoverable OOM.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
#[derive(Clone, Copy)]
struct Slot {
    ticket: u64,
    bytes: usize,
    address: usize,
    used: bool,
    freed: bool,
    retired: bool,
}
const EMPTY: Slot = Slot {
    ticket: 0,
    bytes: 0,
    address: 0,
    used: false,
    freed: false,
    retired: false,
};
struct Observe {
    active: bool,
    slots: [Slot; 2048],
    count: usize,
    allocations: usize,
    unpaid: usize,
    early_refunds: usize,
    unknown_free: usize,
    live: usize,
    peak: usize,
}
std::thread_local! {static OBSERVE:RefCell<Observe>=const {RefCell::new(Observe {active:false,slots:[EMPTY;2048],count:0,allocations:0,unpaid:0,early_refunds:0,unknown_free:0,live:0,peak:0})};}
fn observer_reserve(ticket: u64, bytes: usize) {
    OBSERVE.with(|cell| {
        let mut o = cell.borrow_mut();
        if !o.active {
            return;
        }
        let i = o.count;
        if i == o.slots.len() {
            o.unpaid += 1;
            return;
        }
        o.slots[i] = Slot {
            ticket,
            bytes,
            ..EMPTY
        };
        o.count += 1;
    });
}
fn observer_release(ticket: u64) {
    OBSERVE.with(|cell| {
        let mut o = cell.borrow_mut();
        if !o.active {
            return;
        }
        let count = o.count;
        for i in 0..count {
            if o.slots[i].ticket == ticket && !o.slots[i].retired {
                if o.slots[i].used && !o.slots[i].freed {
                    o.early_refunds += 1;
                }
                o.slots[i].retired = true;
                return;
            }
        }
        o.early_refunds += 1;
    });
}
fn observer_alloc(address: usize, bytes: usize) {
    OBSERVE.with(|cell| {
        let mut o = cell.borrow_mut();
        if !o.active {
            return;
        }
        o.allocations += 1;
        o.live += bytes;
        o.peak = o.peak.max(o.live);
        let count = o.count;
        for i in (0..count).rev() {
            if o.slots[i].bytes == bytes && !o.slots[i].used && !o.slots[i].retired {
                o.slots[i].used = true;
                o.slots[i].address = address;
                return;
            }
        }
        o.unpaid += 1;
    });
}
fn observer_free(address: usize, bytes: usize) {
    OBSERVE.with(|cell| {
        let mut o = cell.borrow_mut();
        if !o.active {
            return;
        }
        o.live = o.live.saturating_sub(bytes);
        let count = o.count;
        for i in 0..count {
            if o.slots[i].used && o.slots[i].address == address && !o.slots[i].freed {
                o.slots[i].freed = true;
                return;
            }
        }
        o.unknown_free += 1;
    });
}
struct SystemObserver;
unsafe impl GlobalAlloc for SystemObserver {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            observer_alloc(p as usize, layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            observer_alloc(p as usize, layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        observer_free(p as usize, layout.size());
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, old, new) };
        if !q.is_null() {
            observer_free(p as usize, old.size());
            observer_alloc(q as usize, new);
        }
        q
    }
}
#[global_allocator]
static ALLOCATOR: SystemObserver = SystemObserver;
struct ObserveGuard;
impl ObserveGuard {
    fn begin() -> Self {
        OBSERVE.with(|cell| {
            let mut o = cell.borrow_mut();
            o.active = true;
            o.slots.fill(EMPTY);
            o.count = 0;
            o.allocations = 0;
            o.unpaid = 0;
            o.early_refunds = 0;
            o.unknown_free = 0;
            o.live = 0;
            o.peak = 0;
        });
        Self
    }
}
impl Drop for ObserveGuard {
    fn drop(&mut self) {
        OBSERVE.with(|cell| cell.borrow_mut().active = false);
    }
}

#[test]
fn complete_mime_percent_and_forgiving_body_are_the_original_algorithm() {
    for input in [
        "data:,Hello%20World!#first",
        "data:TEXT/Plain;ONE=First;one=Second;x=\"a;b\\\"c\";base64,SGVsbG8=",
        " \tD\na\rt\ta:;charset=\"UT;F\\\"8\",%FF%25%zz#fragment ",
        "data:image/svg+xml;bad=\"unfinished;a=b;base64,SGVsbG8",
    ] {
        let plain = crate::DataUrl::process(input).unwrap();
        let expected = plain.decode_to_vec().unwrap();
        let (r, state) = resources(Limits::default(), 0);
        let paid = DataUrl::process(input, r.clone()).unwrap();
        let view = paid.mime_type();
        assert_eq!(view.type_(), plain.mime_type().type_);
        assert_eq!(view.subtype(), plain.mime_type().subtype);
        let actual: Vec<_> = view.parameters().collect();
        let expected_params: Vec<_> = plain
            .mime_type()
            .parameters
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        assert_eq!(actual, expected_params);
        let length = paid.decoded_length().unwrap();
        assert_eq!(length, expected.0.len());
        let mut output = BudgetVec::new(r.clone(), ResourceKind::Scratch);
        output.try_reserve_exact(length).unwrap();
        let fragment = paid.decode_into(length, &mut output).unwrap();
        assert_eq!(output.as_slice(), expected.0.as_slice());
        assert_eq!(fragment.map(|f| f.as_str()), expected.1.map(|f| f.0));
        drop(output);
        drop(paid);
        r.validate_retired().unwrap();
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn count_pass_is_exact_and_does_not_consume_child_admission() {
    let (r, state) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:;base64,SGVsbG8", r.clone()).unwrap();
    assert_eq!(url.decoded_length().unwrap(), 5);
    assert_eq!(r.statistics().children, 0);
    assert_eq!(r.statistics().child_encoded_bytes, 0);
    let bytes = decode("data:;base64,SGVsbG8", &r).unwrap();
    assert_eq!(bytes.as_ref().as_slice(), b"Hello");
    assert_eq!(r.statistics().children, 1);
    assert_eq!(r.statistics().child_encoded_bytes, 5);
    drop(bytes);
    drop(url);
    r.validate_retired().unwrap();
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
}

#[test]
fn actual_large_header_allocations_are_prepaid_and_free_before_refund() {
    let input = format!(
        "data:text/plain;x=\"{}\";base64,SGVsbG8=",
        "a".repeat(1 << 20)
    );
    let (r, state) = resources(Limits::default(), 0);
    let guard = ObserveGuard::begin();
    let body = decode(&input, &r).unwrap();
    let clone = body.clone();
    drop(body);
    assert!(state.live.load(Ordering::SeqCst) > 0);
    drop(clone);
    drop(guard);
    let counts = OBSERVE.with(|cell| {
        let o = cell.borrow();
        (
            o.allocations,
            o.unpaid,
            o.early_refunds,
            o.unknown_free,
            o.live,
            o.peak,
            o.count,
        )
    });
    eprintln!("URL observer allocations/unpaid/early/unknown/live/peak/reserves={counts:?}");
    assert!(counts.0 > 10);
    assert_eq!(counts.1, 0);
    assert_eq!(counts.2, 0);
    assert_eq!(counts.3, 0);
    assert_eq!(counts.4, 0);
    assert_eq!(counts.0, counts.6);
    r.validate_retired().unwrap();
}

#[test]
fn child_limit_and_exact_aggregate_bytes_are_enforced_between_passes() {
    for maximum in [4, 5, 6] {
        let mut limits = Limits::default();
        limits.child_encoded_bytes = maximum;
        let (r, state) = resources(limits, 0);
        let result = decode("data:,Hello", &r);
        assert_eq!(result.is_ok(), maximum >= 5);
        if maximum < 5 {
            assert_eq!(r.failure().unwrap().limit, LimitKind::ChildEncoded);
        }
        drop(result);
        r.validate_retired().unwrap();
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
    }
    let (r, _) = resources(Limits::default(), 0);
    for _ in 0..64 {
        drop(decode("data:,a", &r).unwrap());
    }
    assert_eq!(r.statistics().children, 64);
    assert_eq!(r.statistics().depth, 0);
    assert!(decode("data:,a", &r).is_err());
    assert_eq!(r.failure().unwrap().limit, LimitKind::Children);
    r.validate_retired().unwrap();
}

#[test]
fn real_decoded_children_hit_default_eight_mib_aggregate_boundary() {
    let first = format!("data:,{}", "a".repeat(4 << 20));
    for delta in [-1isize, 0, 1] {
        let second = format!(
            "data:,{}",
            "b".repeat(((4 << 20) as isize + delta) as usize)
        );
        let (r, state) = resources(Limits::default(), 0);
        let initial = decode(&first, &r).unwrap();
        assert_eq!(initial.len(), 4 << 20);
        drop(initial);
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
        assert_eq!(r.statistics().child_encoded_bytes, 4 << 20);
        let result = decode(&second, &r);
        assert_eq!(result.is_ok(), delta <= 0);
        if delta > 0 {
            assert_eq!(r.failure().unwrap().limit, LimitKind::ChildEncoded);
        } else {
            assert_eq!(
                r.statistics().child_encoded_bytes,
                ((8 << 20) as isize + delta) as u64
            );
        }
        drop(result);
        r.validate_retired().unwrap();
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn new_body_buffer_must_belong_to_this_request_and_have_prepaid_capacity() {
    let (r, _) = resources(Limits::default(), 0);
    let (other, _) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:,hello", r.clone()).unwrap();
    let mut wrong = BudgetVec::new(other.clone(), ResourceKind::Scratch);
    wrong.try_reserve_exact(5).unwrap();
    assert!(url.decode_into(5, &mut wrong).is_err());
    assert_eq!(r.failure().unwrap().kind, FailureKind::Invariant);
    assert!(other.failure().is_none());
    drop(url);
    drop(wrong);
    r.validate_retired().unwrap();
    other.validate_retired().unwrap();
    let (r, _) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:,hello", r.clone()).unwrap();
    let mut small = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    assert!(url.decode_into(5, &mut small).is_err());
    assert_eq!(small.capacity(), 0);
    drop(url);
    drop(small);
    r.validate_retired().unwrap();
}

#[test]
fn native_reserve_failures_keep_original_code_and_all_actual_owners_retire() {
    let input = format!(
        "data:text/plain;x=\"{}\",{}",
        "x".repeat(70000),
        "a".repeat(10000)
    );
    let (r, state) = resources(Limits::default(), 0);
    drop(decode(&input, &r).unwrap());
    r.validate_retired().unwrap();
    let count = state.reserves.load(Ordering::SeqCst);
    for fail_at in 1..=count {
        let (r, state) = resources(Limits::default(), fail_at);
        let error = decode(&input, &r).unwrap_err();
        assert!(
            matches!(error,UrlError::Resource(e) if e==Error::native(-1234567)),
            "reserve {}",
            fail_at
        );
        assert_eq!(r.failure(), Some(Error::native(-1234567)));
        assert_eq!(state.live.load(Ordering::SeqCst), 0, "reserve {fail_at}");
        r.validate_retired().unwrap();
    }
    eprintln!("URL actual Native reserve points={count}");
}

#[test]
fn fatal_body_semantic_is_recorded_while_header_lives_before_cleanup_hresult() {
    let (r, state) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:text/plain;x=header;base64,!!!!", r.clone()).unwrap();
    state.late_release.store(true, Ordering::SeqCst);
    let error = url.decoded_length().unwrap_err();
    assert!(matches!(error, UrlError::InvalidBase64(_)));
    semantic(&r, &error);
    drop(url);
    assert_eq!(r.failure().unwrap().kind, FailureKind::MalformedImageData);
    assert_eq!(r.failure().unwrap().native_hr, None);
    assert!(r.statistics().release_failures > 0);
    assert!(r.validate_retired().is_err());
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
}

#[test]
fn successful_body_still_requires_check_after_header_cleanup() {
    let (r, state) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:text/plain;x=header,a", r.clone()).unwrap();
    let mut output = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    output.try_reserve_exact(1).unwrap();
    url.decode_into(1, &mut output).unwrap();
    state.late_release.store(true, Ordering::SeqCst);
    drop(url);
    assert_eq!(r.checkpoint(0), Err(Error::native(-777)));
    drop(output);
    assert!(r.validate_retired().is_err());
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
}

#[test]
fn original_syntax_fallback_does_not_record_a_false_fatal_request() {
    for input in [
        "not-a-url",
        "data:missing-comma",
        "data:text/plain#fragment,bad",
    ] {
        let (r, _) = resources(Limits::default(), 0);
        let error = DataUrl::process(input, r.clone()).unwrap_err();
        assert!(matches!(error, UrlError::Syntax(_)));
        assert!(r.failure().is_none());
        r.validate_retired().unwrap();
    }
    let (r, _) = resources(Limits::default(), 0);
    let url = DataUrl::process("data:invalid MIME,hello", r.clone()).unwrap();
    assert!(url.mime_type().matches("text", "plain"));
    assert!(r.failure().is_none());
    drop(url);
    r.validate_retired().unwrap();
}

#[test]
fn long_no_output_body_loops_cancel_inside_original_decoders_and_fresh_recovers() {
    for byte in [' ', '='] {
        let tail = if byte == ' ' { "YQ==" } else { "" };
        let input = format!(
            "data:text/plain;base64,{}{tail}",
            byte.to_string().repeat(1 << 20)
        );
        let (r, state) = resources(Limits::default(), 0);
        let url = DataUrl::process(&input, r.clone()).unwrap();
        let start = state.callbacks.load(Ordering::SeqCst);
        state.cancel_at.store(start + 3, Ordering::SeqCst);
        let error = url.decoded_length().unwrap_err();
        assert!(matches!(error,UrlError::Resource(e) if e.kind==FailureKind::Cancelled));
        assert_eq!(state.callbacks.load(Ordering::SeqCst), start + 3);
        drop(url);
        r.validate_retired().unwrap();
    }
    let (r, _) = resources(Limits::default(), 0);
    let bytes = decode("data:;base64,YQ==", &r).unwrap();
    assert_eq!(bytes.as_ref().as_slice(), b"a");
    drop(bytes);
    r.validate_retired().unwrap();
}

#[test]
fn long_url_trimming_checks_cancellation_before_header_allocation() {
    let input = format!("data:,{}", " ".repeat(1 << 20));
    let (r, state) = resources(Limits::default(), 0);
    state.cancel_at.store(3, Ordering::SeqCst);
    let error = DataUrl::process(&input, r.clone()).unwrap_err();
    assert!(matches!(error,UrlError::Resource(e) if e.kind==FailureKind::Cancelled));
    assert_eq!(state.callbacks.load(Ordering::SeqCst), 3);
    assert_eq!(state.reserves.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
}

#[test]
fn long_header_cancel_can_happen_after_real_paid_allocation() {
    let input = format!("data:text/plain;x=\"{}\",a", "x".repeat(1 << 20));
    let (r, state) = resources(Limits::default(), 0);
    let url = DataUrl::process(&input, r.clone()).unwrap();
    let first = state.first_reserve_callback.load(Ordering::SeqCst);
    drop(url);
    r.validate_retired().unwrap();
    assert!(first > 1);
    let (r, state) = resources(Limits::default(), 0);
    state.cancel_at.store(first + 2, Ordering::SeqCst);
    let error = DataUrl::process(&input, r.clone()).unwrap_err();
    assert!(matches!(error,UrlError::Resource(e) if e.kind==FailureKind::Cancelled));
    assert!(state.reserves.load(Ordering::SeqCst) > 0);
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
}

#[test]
fn raw_fragment_is_borrowed_and_not_scanned_as_body() {
    let input = format!("data:,hello#{}", "x".repeat(1 << 20));
    let (r, state) = resources(Limits::default(), 0);
    let url = DataUrl::process(&input, r.clone()).unwrap();
    assert_eq!(url.decoded_length().unwrap(), 5);
    let mut output = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    output.try_reserve_exact(5).unwrap();
    let fragment = url.decode_into(5, &mut output).unwrap().unwrap();
    assert_eq!(fragment.as_str().len(), 1 << 20);
    assert_eq!(fragment.as_str().as_ptr(), input.as_bytes()[12..].as_ptr());
    assert!(state.callbacks.load(Ordering::SeqCst) < 30);
    drop(output);
    drop(url);
    r.validate_retired().unwrap();
}

#[test]
fn parameter_index_preserves_mixed_case_first_and_source_order() {
    let (r, _) = resources(Limits::default(), 0);
    let mime = crate::controlled::Mime::parse(
        "image/png;x=first;X=upper-one;x=ignored;X=upper-two;y=second;Y=upper-y;y=ignored",
        r.clone(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        mime.view().parameters().collect::<Vec<_>>(),
        [
            ("x", "first"),
            ("x", "upper-one"),
            ("x", "upper-two"),
            ("y", "second"),
            ("y", "upper-y")
        ]
    );
    drop(mime);
    r.validate_retired().unwrap();
    for order in 0..3 {
        let mut input = std::string::String::from("image/png");
        let mut expected = Vec::new();
        for n in 0..1024 {
            let id = match order {
                0 => n,
                1 => 1023 - n,
                _ => {
                    if n % 2 == 0 {
                        n / 2
                    } else {
                        1023 - n / 2
                    }
                }
            };
            let name = format!("prefix-common-long-{}-{id:05}", "x".repeat(64));
            write!(&mut input, ";{name}={id}").unwrap();
            expected.push((name, id.to_string()));
        }
        let (r, _) = resources(Limits::default(), 0);
        let mime = crate::controlled::Mime::parse(&input, r.clone())
            .unwrap()
            .unwrap();
        for ((name, value), (expected_name, expected_value)) in
            mime.view().parameters().zip(&expected)
        {
            assert_eq!(
                (name, value),
                (expected_name.as_str(), expected_value.as_str())
            );
        }
        assert_eq!(mime.view().parameters().count(), expected.len());
        drop(mime);
        r.validate_retired().unwrap();
    }
}

#[test]
fn twelve_thousand_ordinary_parameters_fit_default_work_without_quadratic_scan() {
    let mut input = std::string::String::from("image/png");
    for n in 0..12000 {
        write!(&mut input, ";x{n:05}=1").unwrap();
    }
    assert_eq!(input.len(), 108009);
    let (r, state) = resources(Limits::default(), 0);
    let mime = crate::controlled::Mime::parse(&input, r.clone())
        .unwrap()
        .unwrap();
    assert_eq!(mime.view().parameters().count(), 12000);
    assert!(
        r.statistics().work < 8_000_000,
        "work={}",
        r.statistics().work
    );
    eprintln!(
        "MIME actual12000 parameters work={} peak={}",
        r.statistics().work,
        state.peak.load(Ordering::SeqCst)
    );
    drop(mime);
    r.validate_retired().unwrap();
    let (r, state) = resources(Limits::default(), 0);
    state.cancel_at.store(25, Ordering::SeqCst);
    let error = crate::controlled::Mime::parse(&input, r.clone()).unwrap_err();
    assert!(matches!(error,UrlError::Resource(e) if e.kind==FailureKind::Cancelled));
    assert!(state.reserves.load(Ordering::SeqCst) > 10);
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
    r.validate_retired().unwrap();
}

#[test]
fn actual_parameter_index_growth_is_paid_and_every_native_reserve_failure_retires() {
    let mut input = std::string::String::from("image/png");
    for n in 0..128 {
        write!(&mut input, ";name{n:05}=value").unwrap();
    }
    let (r, state) = resources(Limits::default(), 0);
    let guard = ObserveGuard::begin();
    let mime = crate::controlled::Mime::parse(&input, r.clone())
        .unwrap()
        .unwrap();
    assert_eq!(mime.view().parameters().count(), 128);
    drop(mime);
    drop(guard);
    let counts = OBSERVE.with(|cell| {
        let o = cell.borrow();
        (
            o.allocations,
            o.unpaid,
            o.early_refunds,
            o.unknown_free,
            o.live,
            o.peak,
            o.count,
        )
    });
    eprintln!("MIME index observer alloc/unpaid/early/unknown/live/peak/reserves={counts:?}");
    assert_eq!((counts.1, counts.2, counts.3, counts.4), (0, 0, 0, 0));
    assert_eq!(counts.0, counts.6);
    assert!(counts.0 > 256);
    r.validate_retired().unwrap();
    let points = state.reserves.load(Ordering::SeqCst);
    for fail_at in 1..=points {
        let (r, state) = resources(Limits::default(), fail_at);
        let error = crate::controlled::Mime::parse(&input, r.clone()).unwrap_err();
        assert!(
            matches!(error,UrlError::Resource(e) if e==Error::native(-1234567)),
            "point {}",
            fail_at
        );
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
        r.validate_retired().unwrap();
    }
    eprintln!("MIME index actual reserve failures={points}");
}

#[test]
fn temporary_parameter_index_release_failure_is_checked_before_mime_success() {
    let (r, state) = resources(Limits::default(), 0);
    // Four initial repr(C) nodes: three usize indices plus a height rounded to
    // pointer alignment. The single-parameter parse does not grow this block.
    state
        .fail_release_bytes
        .store(16 * core::mem::size_of::<usize>(), Ordering::SeqCst);
    let error = crate::controlled::Mime::parse("image/png;x=1", r.clone()).unwrap_err();
    assert!(matches!(error,UrlError::Resource(e) if e==Error::native(-777)));
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(
        r.statistics().scratch_bytes,
        16 * core::mem::size_of::<usize>() as u64
    );
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
    assert!(r.validate_retired().is_err());
}
