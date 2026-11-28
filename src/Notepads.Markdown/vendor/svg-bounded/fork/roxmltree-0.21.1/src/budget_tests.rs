// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

const TEST_TICKETS: usize = 16_384;

struct State {
    next: AtomicU64,
    live: AtomicU64,
    peak: AtomicU64,
    maximum_request: AtomicU64,
    reserve_calls: AtomicU64,
    checkpoints: AtomicU64,
    checkpoint_units: AtomicU64,
    fail_checkpoint: AtomicU64,
    hr: AtomicU64,
    release_hr: AtomicU64,
    bytes: [AtomicU64; TEST_TICKETS],
}
impl State {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            live: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            maximum_request: AtomicU64::new(0),
            reserve_calls: AtomicU64::new(0),
            checkpoints: AtomicU64::new(0),
            checkpoint_units: AtomicU64::new(0),
            fail_checkpoint: AtomicU64::new(0),
            hr: AtomicU64::new(0),
            release_hr: AtomicU64::new(0),
            bytes: [const { AtomicU64::new(0) }; TEST_TICKETS],
        }
    }
}
struct TestBackend(Arc<State>);
impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
        self.0.reserve_calls.fetch_add(1, Ordering::Relaxed);
        let hr = self.0.hr.load(Ordering::Relaxed);
        if hr != 0 {
            return Err(ResourceError::native(hr as u32 as i32));
        }
        let next = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        assert!(
            (next as usize) < TEST_TICKETS,
            "pure backend observation storage exhausted"
        );
        self.0.bytes[next as usize].store(bytes as u64, Ordering::Relaxed);
        let live = self.0.live.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
        self.0.peak.fetch_max(live, Ordering::Relaxed);
        self.0
            .maximum_request
            .fetch_max(bytes as u64, Ordering::Relaxed);
        Ticket::new(1, next as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
        let hr = self.0.release_hr.load(Ordering::Relaxed);
        if hr != 0 {
            return Err(ResourceError::native(hr as u32 as i32));
        }
        let bytes = self.0.bytes[u64::from(ticket.slot()) as usize].swap(0, Ordering::Relaxed);
        self.0.live.fetch_sub(bytes, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), ResourceError> {
        self.0.checkpoint_units.fetch_add(work, Ordering::Relaxed);
        let call = self.0.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        let fail = self.0.fail_checkpoint.load(Ordering::Relaxed);
        if fail != 0 && call >= fail {
            return Err(ResourceError::native(0x80004004u32 as i32));
        }
        Ok(())
    }
}
fn setup(limits: Limits) -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State::new());
    let resources = ResourcesRef::new_prepaid(
        TestBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        limits,
    );
    (resources, state)
}
fn parse<'a>(source: &'a str, resources: &ResourcesRef) -> Result<Document<'a>, Error> {
    Document::parse_with_options(
        source,
        ParsingOptions {
            resources: Some(resources.clone()),
            allow_dtd: false,
            nodes_limit: 65_536,
            ..Default::default()
        },
    )
}
fn boundary(source: &str, kind: LimitKind, actual: u64, set: fn(&mut Limits, u64)) {
    for limit in [actual - 1, actual, actual + 1] {
        let mut limits = Limits::default();
        set(&mut limits, limit);
        let (resources, state) = setup(limits);
        let result = parse(source, &resources);
        assert_eq!(
            result.is_ok(),
            limit >= actual,
            "kind={:?} limit={}",
            kind,
            limit
        );
        if limit < actual {
            assert_eq!(resources.failure().unwrap().limit, kind);
        }
        drop(result);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn xml_nodes_limit_minus_equal_plus_counts_document_root() {
    boundary("<e><x/></e>", LimitKind::XmlNodes, 3, |l, n| {
        l.xml_nodes = n
    });
}
#[test]
fn xml_attribute_limit_minus_equal_plus_includes_namespace_declarations() {
    boundary(
        "<e xmlns='urn:test' a='1'/>",
        LimitKind::XmlAttributes,
        2,
        |l, n| l.xml_attributes = n,
    );
}
#[test]
fn xml_per_element_limit_minus_equal_plus_precedes_normalization() {
    boundary(
        "<e xmlns='urn:test' a='1' b='2'/>",
        LimitKind::XmlElementAttributes,
        3,
        |l, n| l.xml_element_attributes = n,
    );
}
#[test]
fn xml_structure_depth_limit_minus_equal_plus_includes_empty_elements() {
    boundary("<e><a><b/></a></e>", LimitKind::XmlDepth, 3, |l, n| {
        l.xml_depth = n
    });
}
#[test]
fn namespaces_and_references_keep_paid_document_storage_until_drop() {
    let (resources, state) = setup(Limits::default());
    let doc = parse(
        "<e xmlns:a='u1' xmlns:b='u2'><a:x xmlns:c='u3' b:y='1'><c:z/></a:x></e>",
        &resources,
    )
    .unwrap();
    assert_eq!(doc.root_element().namespaces().len(), 2);
    assert!(state.live.load(Ordering::Relaxed) > 0);
    assert!(resources.statistics().scratch_bytes > 0);
    drop(doc);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn total_attribute_count_does_not_reset_at_new_elements() {
    boundary(
        "<e a='1'><x b='2'/></e>",
        LimitKind::XmlAttributes,
        2,
        |l, n| l.xml_attributes = n,
    );
}
#[test]
fn backend_native_hr_is_first_and_no_unpaid_dom_prefix_survives() {
    let (resources, state) = setup(Limits::default());
    state.hr.store(0x8007000eu32 as u64, Ordering::Relaxed);
    assert!(matches!(parse("<e/>", &resources), Err(Error::Resource(_))));
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(0x8007000eu32 as i32)
    );
    assert_eq!(state.reserve_calls.load(Ordering::Relaxed), 1);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn actual_event_checkpoint_cancels_a_live_parse_and_fresh_scope_recovers() {
    let source = std::format!("<e>{}</e>", "<a/>".repeat(300));
    let (resources, state) = setup(Limits::default());
    // entry + input chunk + first event batch; cancellation occurs at the
    // second event batch with actual DOM allocations already alive.
    state.fail_checkpoint.store(4, Ordering::Relaxed);
    assert!(parse(&source, &resources).is_err());
    assert!(state.reserve_calls.load(Ordering::Relaxed) > 0);
    assert_eq!(state.checkpoints.load(Ordering::Relaxed), 4);
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(0x80004004u32 as i32)
    );
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
    let (fresh, state) = setup(Limits::default());
    drop(parse("<e/>", &fresh).unwrap());
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    fresh.validate_retired().unwrap();
}
#[test]
fn semantic_xml_failure_precedes_late_release_failure() {
    let (resources, state) = setup(Limits::default());
    state
        .release_hr
        .store(0x8000ffffu32 as u64, Ordering::Relaxed);
    assert!(parse("<e><x></wrong></e>", &resources).is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Decode);
    assert_eq!(resources.failure().unwrap().native_hr, None);
    assert!(resources.statistics().release_failures > 0);
    assert!(resources.validate_retired().is_err());
}
#[test]
fn strict_controlled_route_refuses_dtd_and_external_entity_resolver_before_dom() {
    let (resources, state) = setup(Limits::default());
    let result = Document::parse_with_options(
        "<!DOCTYPE e><e/>",
        ParsingOptions {
            resources: Some(resources.clone()),
            allow_dtd: true,
            ..Default::default()
        },
    );
    assert!(result.is_err());
    assert_eq!(state.reserve_calls.load(Ordering::Relaxed), 0);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Unsupported);
    resources.validate_retired().unwrap();
}
#[test]
fn scratch_refusal_is_before_dom_allocation_and_releases_earlier_arrays() {
    let mut limits = Limits::default();
    limits.scratch_bytes = 128;
    let (resources, state) = setup(limits);
    assert!(parse("<e/>", &resources).is_err());
    assert_eq!(resources.failure().unwrap().limit, LimitKind::Scratch);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn budgeted_and_unbudgeted_routes_keep_same_xml_semantics() {
    let source = "<e xmlns:a='u' a:b='&amp;'><a:x/>one<![CDATA[two]]>three</e>";
    let expected = Document::parse(source).unwrap();
    let (resources, _) = setup(Limits::default());
    let actual = parse(source, &resources).unwrap();
    assert_eq!(expected.descendants().count(), actual.descendants().count());
    assert_eq!(
        expected.root_element().attribute(("u", "b")),
        actual.root_element().attribute(("u", "b"))
    );
    assert_eq!(
        expected.root_element().last_child().unwrap().text(),
        actual.root_element().last_child().unwrap().text()
    );
    drop(actual);
    resources.validate_retired().unwrap();
}

#[test]
fn actual_single_long_text_token_cancels_before_materializing_text_node() {
    let source = std::format!("<e><d>{}</d></e>", "=".repeat(3 * 1024 * 1024));
    let (resources, state) = setup(Limits::default());
    // entry(1), first actual scan credit(2), first event credit(3),
    // second actual scan credit(4) while the same 3MiB Text token is scanned.
    state.fail_checkpoint.store(4, Ordering::Relaxed);
    assert!(parse(&source, &resources).is_err());
    assert!(state.reserve_calls.load(Ordering::Relaxed) > 0);
    assert_eq!(state.checkpoints.load(Ordering::Relaxed), 4);
    assert!(state.checkpoint_units.load(Ordering::Relaxed) < source.len() as u64);
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(0x80004004u32 as i32)
    );
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn many_short_normalization_streams_share_request_scanning_credit() {
    // Match DOM shape and one-byte text lengths so node-array migration is the
    // same in both requests. The contract under test is normalization overhead,
    // independent of that already-paid DOM work.
    let plain_source = std::format!("<e>{}</e>", "<x>A</x>".repeat(5000));
    let (plain_resources, plain_state) = setup(Limits::default());
    let plain_doc = parse(&plain_source, &plain_resources).unwrap();
    assert_eq!(plain_doc.descendants().count(), 10_002);
    let plain_work = plain_state.checkpoint_units.load(Ordering::Relaxed);
    drop(plain_doc);
    assert_eq!(plain_state.live.load(Ordering::Relaxed), 0);
    plain_resources.validate_retired().unwrap();

    let source = std::format!("<e>{}</e>", "<x>&amp;</x>".repeat(5000));
    let (resources, state) = setup(Limits::default());
    let doc = parse(&source, &resources).unwrap();
    assert_eq!(doc.descendants().count(), 10_002);
    let total_work = state.checkpoint_units.load(Ordering::Relaxed);
    let normalization_work = total_work.checked_sub(plain_work).unwrap();
    assert!(normalization_work < source.len() as u64 * 16 + 65_536);
    std::println!(
        "SHORT_STREAM_WORK plain={plain_work} normalized={total_work} extra={normalization_work}"
    );
    drop(doc);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn controlled_normalized_storage_is_paid_and_clone_retains_only_one_allocation() {
    let (resources, state) = setup(Limits::default());
    let doc = parse("<e a='中&#xA;文'>A&amp;😀</e>", &resources).unwrap();
    assert!(matches!(
        doc.root_element()
            .attributes()
            .next()
            .unwrap()
            .value_storage(),
        StringStorage::Paid(_)
    ));
    let original = doc.root_element().text_storage().unwrap();
    let before = state.reserve_calls.load(Ordering::Relaxed);
    let copy = original.clone();
    match (original, &copy) {
        (StringStorage::Paid(a), StringStorage::Paid(b)) => assert!(a.ptr_eq(b)),
        _ => panic!("controlled normalization escaped paid storage"),
    }
    assert_eq!(state.reserve_calls.load(Ordering::Relaxed), before);
    drop(doc);
    assert_eq!(copy.as_str(), "A&😀");
    assert!(state.live.load(Ordering::Relaxed) > 0);
    assert!(resources.validate_retired().is_err());
    // A retirement check records a terminal invariant, so a fresh scope below
    // checks the normal release path without deliberately poisoning it.
    drop(copy);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
}

#[test]
fn cloned_paid_string_is_fully_retired_after_the_last_handle() {
    let (resources, state) = setup(Limits::default());
    let doc = parse("<e>&amp;中文😀</e>", &resources).unwrap();
    let copied = doc.root_element().text_storage().unwrap().clone();
    drop(doc);
    assert!(state.live.load(Ordering::Relaxed) > 0);
    drop(copied);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn raw_text_and_attributes_still_borrow_the_source() {
    let (resources, state) = setup(Limits::default());
    let doc = parse("<e a='原文'>English 中文😀</e>", &resources).unwrap();
    assert!(matches!(
        doc.root_element().text_storage(),
        Some(StringStorage::Borrowed(_))
    ));
    assert!(matches!(
        doc.root_element()
            .attributes()
            .next()
            .unwrap()
            .value_storage(),
        StringStorage::Borrowed(_)
    ));
    drop(doc);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn normalization_preserves_unicode_and_cr_rules_at_every_staging_boundary() {
    for length in 4089..4103 {
        let prefix = "a".repeat(length);
        let source =
            std::format!("<e a='{prefix}\r\n中&#xD;&#xA;\t末'>{prefix}\r\n中&#x1F600;\r末</e>");
        let (resources, state) = setup(Limits::default());
        let doc = parse(&source, &resources).unwrap();
        assert_eq!(
            doc.root_element().text().unwrap(),
            std::format!("{prefix}\n中😀\n末")
        );
        assert_eq!(
            doc.root_element().attribute("a").unwrap(),
            std::format!("{prefix} 中\r\n 末")
        );
        drop(doc);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn cdata_and_adjacent_text_merge_keep_reference_cr_verbatim() {
    for length in [0, 4091, 4094, 4095, 4096, 8191] {
        let prefix = "a".repeat(length);
        let source =
            std::format!("<e>{prefix}<![CDATA[\r\n中\r末]]>Z&#x1F600;&#13;<![CDATA[\n]]>&#xD;</e>");
        let (resources, state) = setup(Limits::default());
        let doc = parse(&source, &resources).unwrap();
        assert_eq!(
            doc.root_element().text().unwrap(),
            std::format!("{prefix}\n中\n末Z😀\r\n\r")
        );
        assert_eq!(doc.root_element().children().count(), 1);
        drop(doc);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn normalization_peak_limit_minus_equal_plus_and_fresh_recovery() {
    let source = std::format!(
        "<e a='{}\r中&amp;末'>a<![CDATA[b\rc]]>&amp;d</e>",
        "x".repeat(12_000)
    );
    let (resources, state) = setup(Limits::default());
    drop(parse(&source, &resources).unwrap());
    let peak = state.peak.load(Ordering::Relaxed);
    assert!(peak > 12_000);
    resources.validate_retired().unwrap();
    for limit in [peak - 1, peak, peak + 1] {
        let mut limits = Limits::default();
        limits.scratch_bytes = limit;
        let (resources, state) = setup(limits);
        let result = parse(&source, &resources);
        assert_eq!(result.is_ok(), limit >= peak);
        if limit < peak {
            assert_eq!(resources.failure().unwrap().limit, LimitKind::Scratch);
        }
        drop(result);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
    let (fresh, _) = setup(Limits::default());
    drop(parse("<e>&amp;</e>", &fresh).unwrap());
    fresh.validate_retired().unwrap();
}

#[test]
fn cancellation_during_normalized_buffer_ownership_retires_every_ticket() {
    let source = std::format!("<e>{}&#x1F600;</e>", "x".repeat(90_000));
    let (baseline, state) = setup(Limits::default());
    drop(parse(&source, &baseline).unwrap());
    let calls = state.checkpoints.load(Ordering::Relaxed);
    assert!(
        calls < source.len() as u64 / 128,
        "output checkpoint regressed to per-byte calls"
    );
    baseline.validate_retired().unwrap();
    let (resources, state) = setup(Limits::default());
    state.fail_checkpoint.store(calls, Ordering::Relaxed);
    assert!(parse(&source, &resources).is_err());
    assert!(state.maximum_request.load(Ordering::Relaxed) >= 65_536);
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(0x80004004u32 as i32)
    );
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
    let (fresh, _) = setup(Limits::default());
    drop(parse(&source, &fresh).unwrap());
    fresh.validate_retired().unwrap();
}

#[test]
fn controlled_named_xml_errors_do_not_materialize_unpaid_error_strings() {
    let name = "a".repeat(8192);
    let sources = [
        std::format!("<e>&{name};</e>"),
        std::format!("<e a='&{name};'/>"),
        std::format!("<e {name}='a' {name}='b'/>"),
        std::format!("<e xmlns:{name}='a' xmlns:{name}='b'/>"),
        std::format!("<{name}></different>"),
        std::format!("<{name}:e/>"),
    ];
    for source in sources {
        let (resources, state) = setup(Limits::default());
        assert!(matches!(
            parse(&source, &resources),
            Err(Error::Resource(_))
        ));
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Decode);
        assert!(state.maximum_request.load(Ordering::Relaxed) < 8192);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
    assert!(matches!(
        Document::parse("<e>&missing;</e>"),
        Err(Error::UnknownEntityReference(_, _))
    ));
}
