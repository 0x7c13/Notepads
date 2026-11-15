// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real SVG cascade/draft inputs, captured before the intentional font-host rejection.
use super::*;
use std::cell::RefCell;

thread_local! {
    static FAMILIES: RefCell<Option<Vec<Vec<u8>>>> = const { RefCell::new(None) };
}

unsafe extern "C" fn capture_family(_: u64, draft: *const abi::Draft, _: *mut abi::Plan) -> i32 {
    let outcome = std::panic::catch_unwind(|| {
        if draft.is_null() {
            return;
        }
        // SAFETY: the actual producer borrows this validated draft and its arrays
        // for this callback only. Copies stay in the test's own thread-local sink.
        let draft = unsafe { &*draft };
        if draft.styles.is_null() || draft.strings.is_null() || draft.style_count > 65535 {
            return;
        }
        let styles =
            unsafe { std::slice::from_raw_parts(draft.styles, draft.style_count as usize) };
        let strings =
            unsafe { std::slice::from_raw_parts(draft.strings, draft.strings_length as usize) };
        let mut found = Vec::new();
        for style in styles {
            let start = style.family_start as usize;
            let Some(end) = start.checked_add(style.family_length as usize) else {
                return;
            };
            let Some(value) = strings.get(start..end) else {
                return;
            };
            found.push(value.to_vec());
        }
        FAMILIES.with(|sink| {
            if let Ok(mut sink) = sink.try_borrow_mut() {
                *sink = Some(found);
            }
        });
    });
    if outcome.is_err() { -109 } else { -101 }
}

fn captured(source_family: &str) -> Option<Vec<Vec<u8>>> {
    let host = Arc::new(TestHost::default());
    let mut font_callbacks = callbacks();
    font_callbacks.plan = Some(capture_family);
    let mut job =
        OwnedJob::new(identity(), font_callbacks, host.clone(), Limits::default()).unwrap();
    let source_family = source_family
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='100' height='40'><text x='2' y='25' font-family=\"{source_family}\">A中e&#x301;</text></svg>"
    );
    FAMILIES.with(|sink| *sink.borrow_mut() = None);
    let result = job.prepare(source.as_bytes());
    let found = FAMILIES.with(|sink| sink.borrow_mut().take());
    finish(&mut job, &host);
    assert!(
        result.is_err(),
        "The test host deliberately returns -101 after capture"
    );
    found
}

#[test]
fn family_transport_real_source_preserves_order_and_generic_kind() {
    for (source, expected) in [
        ("Arial, sans-serif", "\"Arial\", sans-serif"),
        ("serif", "serif"),
        ("SERIF", "serif"),
        ("Sans-Serif", "sans-serif"),
        ("\"serif\"", "\"serif\""),
        ("\"SERIF\"", "\"SERIF\""),
        (
            "Courier New, Arial, monospace",
            "\"Courier New\", \"Arial\", monospace",
        ),
        ("équipe, serif", "\"équipe\", serif"),
    ] {
        assert_eq!(
            captured(source),
            Some(vec![expected.as_bytes().to_vec()]),
            "{source}"
        );
    }
}

#[test]
fn family_transport_real_source_decodes_escapes_once() {
    for (source, expected) in [
        (r"Ar\69 al, s\61 ns-serif", "\"Arial\", sans-serif"),
        (r"serif\20 face, monospace", "\"serif face\", monospace"),
        (r#""A,B", "C\\D", monospace"#, r#""A,B", "C\\D", monospace"#),
        (r#""A\"B", serif"#, r#""A\"B", serif"#),
        (r#""A\1 B", monospace"#, r#""A\000001 B", monospace"#),
        (r#""A\0 B", serif"#, "\"A\u{fffd}B\", serif"),
    ] {
        assert_eq!(
            captured(source),
            Some(vec![expected.as_bytes().to_vec()]),
            "{source}"
        );
    }
}

#[test]
fn family_transport_legacy_plain_single_name_bytes_stay_exact() {
    assert_eq!(captured("Segoe UI"), Some(vec![b"\"Segoe UI\"".to_vec()]));
}

#[derive(Default)]
struct EncodingHost {
    inner: TestHost,
    calls: AtomicU64,
    fail_on: AtomicU64,
}

impl Host for EncodingHost {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.inner.reserve(kind, bytes)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        self.inner.allocation_committed(ticket, bytes)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.inner.release_after_free(ticket)
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        assert!(
            work <= 4096,
            "The family codec must bound its input work batches"
        );
        let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.fail_on.load(Ordering::Relaxed) {
            Err(Error::native(-1814))
        } else {
            self.inner.checkpoint(work)
        }
    }
}

#[test]
fn family_transport_failed_checkpoint_discards_partial_pool_and_preserves_first_fault() {
    let host = Arc::new(EncodingHost::default());
    let context =
        crate::memory::Context::new(callbacks(), host.clone(), Arc::new(AtomicBool::new(false)))
            .unwrap();
    let resources = context.make_resources(Limits::default());
    let processing = usvg::ProcessingContext::with_resources(resources.clone());
    let families = [
        usvg::FontFamily::try_named_in("A\\B", &processing).unwrap(),
        usvg::FontFamily::Serif,
    ];
    let mut strings =
        markdown_svg_budget::BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    strings.try_reserve_exact(64).unwrap();
    host.calls.store(0, Ordering::Relaxed);
    host.fail_on.store(2, Ordering::Relaxed);
    assert!(crate::family::append(&context, &families, &mut strings).is_err());
    assert_eq!(strings.as_slice(), b"\"");
    assert_eq!(resources.failure(), Some(Error::native(-1814)));
    drop(strings);
    drop(families);
    drop(processing);
    resources.validate_retired().unwrap();
    assert!(host.inner.live.lock().unwrap().is_empty());
}

#[test]
fn family_transport_new_pool_admission_failure_frees_storage_before_refund() {
    let host = Arc::new(TestHost::default());
    let context =
        crate::memory::Context::new(callbacks(), host.clone(), Arc::new(AtomicBool::new(false)))
            .unwrap();
    let resources = context.make_resources(Limits::default());
    let processing = usvg::ProcessingContext::with_resources(resources.clone());
    let families = [
        usvg::FontFamily::try_named_in("A\"B", &processing).unwrap(),
        usvg::FontFamily::Monospace,
    ];
    let capacity = crate::family::encoded_len(&context, &families).unwrap();
    let mut strings =
        markdown_svg_budget::BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    host.commit_hr.store((-1815i32) as u32, Ordering::Relaxed);
    start_events();
    assert!(strings.try_reserve_exact(capacity).is_err());
    let events = stop_events();
    assert_eq!(resources.failure(), Some(Error::native(-1815)));
    let physical_free = events
        .iter()
        .position(|event| event.tag == 2 && event.bytes == capacity)
        .unwrap();
    let refund = events.iter().position(|event| event.tag == 5).unwrap();
    assert!(physical_free < refund);
    drop(strings);
    drop(families);
    drop(processing);
    resources.validate_retired().unwrap();
    assert!(host.live.lock().unwrap().is_empty());
}
