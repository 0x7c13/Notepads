// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual controlled DOM namespace regressions. Observer storage belongs only to the test.
use roxmltree::{Document, ParsingOptions};
use std::{format, string::String, vec::Vec};
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
#[derive(Default)]
struct Account {
    next: AtomicU32,
    live: Mutex<std::collections::BTreeMap<u32, usize>>,
    work: AtomicU64,
    reject_after: AtomicU64,
    cancelled: AtomicBool,
    rejected: AtomicBool,
    reserve_after_rejection: AtomicU64,
    fail_cleanup: AtomicBool,
}
struct NamespaceBackend(Arc<Account>);
impl Backend for NamespaceBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        if self.0.rejected.load(Ordering::Relaxed) {
            self.0
                .reserve_after_rejection
                .fetch_add(1, Ordering::Relaxed);
        }
        let slot = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.live.lock().unwrap().insert(slot, bytes);
        Ticket::new(1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(self.0.live.lock().unwrap()[&ticket.slot()], bytes);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        assert!(self.0.live.lock().unwrap().remove(&ticket.slot()).is_some());
        if self.0.fail_cleanup.load(Ordering::Relaxed) && self.0.rejected.load(Ordering::Relaxed) {
            Err(Error::native(-19132))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let total = self.0.work.fetch_add(work, Ordering::Relaxed) + work;
        let reject_after = self.0.reject_after.load(Ordering::Relaxed);
        if reject_after != 0 && total >= reject_after {
            self.0.rejected.store(true, Ordering::Relaxed);
            Err(if self.0.cancelled.load(Ordering::Relaxed) {
                Error::new(FailureKind::Cancelled, LimitKind::None)
            } else {
                Error::native(-19131)
            })
        } else {
            Ok(())
        }
    }
}
fn setup() -> (ResourcesRef, Arc<Account>) {
    setup_with_limits(Limits::default())
}
fn setup_with_limits(limits: Limits) -> (ResourcesRef, Arc<Account>) {
    let account = Arc::new(Account::default());
    (
        ResourcesRef::new_prepaid(
            NamespaceBackend(account.clone()),
            FirstFailureRef::new_prepaid(),
            limits,
        ),
        account,
    )
}
fn parse<'a>(source: &'a str, resources: &ResourcesRef) -> Result<Document<'a>, roxmltree::Error> {
    Document::parse_with_options(
        source,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: 65536,
            resources: Some(resources.clone()),
            ..Default::default()
        },
    )
}
fn nested(depth: usize, names: usize) -> String {
    let mut source = String::from("<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'>");
    for d in 1..=depth {
        source.push_str("<g");
        for n in 0..names {
            source.push_str(&format!(" xmlns:n{d:03}_{n:03}='x'"));
        }
        source.push('>');
    }
    source.push_str("<rect width='1' height='1'/>");
    for _ in 0..depth {
        source.push_str("</g>");
    }
    source.push_str("</svg>");
    source
}
#[test]
fn default_work_limit_rejects_actual_inherited_namespace_cost() {
    // 156191 source bytes, 67 nodes and <=128 declarations/element all fit the
    // original defaults. Before the fix this performed 732825600 exists comparisons
    // while admitting only 2302208 work units. The work ceiling now stops the DOM.
    let source = nested(64, 128);
    assert_eq!(source.len(), 156191);
    let (resources, account) = setup();
    let result = parse(&source, &resources);
    assert!(result.is_err());
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Quota, LimitKind::Work))
    );
    drop(result);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}
#[test]
fn prefix_redefinition_namespace_order_and_escaped_uri_keep_original_semantics() {
    let source="<a xmlns='u0' xmlns:x='u1' xmlns:y='u2'><x:b y:q='1' x:q='2'><c xmlns='u3' xmlns:x='u4' x:r='3'/><y:d/></x:b></a>";
    let (resources, account) = setup();
    let doc = parse(source, &resources).unwrap();
    let elements: Vec<_> = doc.descendants().filter(|n| n.is_element()).collect();
    let actual: Vec<_> = elements
        .iter()
        .map(|n| (n.tag_name().namespace(), n.tag_name().name()))
        .collect();
    assert_eq!(
        actual,
        [
            (Some("u0"), "a"),
            (Some("u1"), "b"),
            (Some("u3"), "c"),
            (Some("u2"), "d")
        ]
    );
    let inherited: Vec<_> = elements[2]
        .namespaces()
        .map(|n| (n.name(), n.uri()))
        .collect();
    assert_eq!(
        inherited,
        [(None, "u3"), (Some("x"), "u4"), (Some("y"), "u2")]
    );
    assert_eq!(elements[1].attribute(("u2", "q")), Some("1"));
    assert_eq!(elements[1].attribute(("u1", "q")), Some("2"));
    assert_eq!(elements[2].attribute(("u4", "r")), Some("3"));
    drop(elements);
    drop(doc);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    let (resources, account) = setup();
    let doc = parse("<a xmlns:x='&amp;u'><x:b/></a>", &resources).unwrap();
    assert_eq!(
        doc.root_element()
            .first_element_child()
            .unwrap()
            .tag_name()
            .namespace(),
        Some("&u")
    );
    drop(doc);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}
#[test]
fn long_common_prefix_keys_and_paid_reverse_insertion_preserve_identity() {
    let long = "a".repeat(100000);
    let source = format!("<a xmlns:{long}x='u1' xmlns:{long}y='u2'><{long}x:b {long}y:q='v'/></a>");
    let (resources, account) = setup();
    let doc = parse(&source, &resources).unwrap();
    let child = doc.root_element().first_element_child().unwrap();
    assert_eq!(child.tag_name().namespace(), Some("u1"));
    assert_eq!(child.attribute(("u2", "q")), Some("v"));
    drop(doc);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    let mut source = String::from("<a>");
    for n in (0..2000).rev() {
        source.push_str(&format!("<b xmlns:n{n:04}='u'><n{n:04}:c/></b>"));
    }
    source.push_str("</a>");
    let (resources, account) = setup();
    let doc = parse(&source, &resources).unwrap();
    for (index, node) in doc
        .root_element()
        .children()
        .filter(|n| n.is_element())
        .enumerate()
    {
        let prefix = format!("n{:04}", 1999 - index);
        assert_eq!(node.lookup_prefix("u"), Some(prefix.as_str()));
        assert_eq!(
            node.first_element_child().unwrap().tag_name().namespace(),
            Some("u")
        );
    }
    drop(doc);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}
#[test]
fn actual_namespace_cancel_and_native_faults_preserve_first_cause_and_stop_new_storage() {
    let source = nested(64, 128);
    for cancelled in [false, true] {
        for fail_cleanup in [false, true] {
            let (resources, account) = setup();
            account.reject_after.store(20000000, Ordering::Relaxed);
            account.cancelled.store(cancelled, Ordering::Relaxed);
            account.fail_cleanup.store(fail_cleanup, Ordering::Relaxed);
            let result = parse(&source, &resources);
            assert!(result.is_err());
            let expected = if cancelled {
                Error::new(FailureKind::Cancelled, LimitKind::None)
            } else {
                Error::native(-19131)
            };
            assert_eq!(resources.failure(), Some(expected));
            drop(result);
            assert_eq!(resources.failure(), Some(expected));
            assert!(account.live.lock().unwrap().is_empty());
            assert_eq!(account.reserve_after_rejection.load(Ordering::Relaxed), 0);
            if fail_cleanup {
                assert!(resources.statistics().release_failures > 0);
                assert_eq!(resources.validate_retired(), Err(expected));
            } else {
                resources.validate_retired().unwrap();
            }
        }
    }
}

fn attribute_source(count: usize, name_len: usize, common_prefix: bool) -> String {
    let prefix = "a".repeat(name_len - 4);
    let mut source =
        String::from("<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><g");
    for index in 0..count {
        let suffix = format!("x{index:03}");
        source.push(' ');
        if common_prefix {
            source.push_str(&prefix);
            source.push_str(&suffix);
        } else {
            source.push_str(&suffix);
            source.push_str(&prefix);
        }
        source.push_str("=''");
    }
    source.push_str("><rect width='1' height='1'/></g></svg>");
    source
}

fn accepted_attribute_work(source: &str) -> u64 {
    let (resources, account) = setup();
    let document = parse(source, &resources).unwrap();
    assert_eq!(
        document
            .descendants()
            .filter(|node| node.is_element())
            .count(),
        3
    );
    drop(document);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    account.work.load(Ordering::Relaxed)
}

#[test]
fn attribute_long_common_prefixes_exhaust_default_work() {
    // The complete input, 256 attributes on g, 261 total attributes and four
    // DOM nodes all fit the defaults. Previously its 1,067,197,440 common-prefix
    // bytes were compared while the request reported only 33,554,948 work.
    let source = attribute_source(256, 32_700, true);
    assert_eq!(source.len(), 8_372_326);
    assert!(source.len() as u64 <= Limits::default().input_bytes);
    let (resources, account) = setup();
    let result = parse(&source, &resources);
    assert!(result.is_err());
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Quota, LimitKind::Work))
    );
    drop(result);
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    accepted_attribute_work(&attribute_source(256, 32_700, false));
}

#[test]
fn attribute_work_limit_keeps_early_difference_control_accepted() {
    let early = attribute_source(32, 12_000, false);
    let common = attribute_source(32, 12_000, true);
    assert_eq!(early.len(), common.len());
    let early_work = accepted_attribute_work(&early);
    let common_work = accepted_attribute_work(&common);
    assert!(common_work > early_work + 1_000_000);
    let limits = Limits {
        work: early_work + 65_536,
        ..Limits::default()
    };
    let (resources, account) = setup_with_limits(limits);
    drop(parse(&early, &resources).unwrap());
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    let (resources, account) = setup_with_limits(limits);
    assert!(parse(&common, &resources).is_err());
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Quota, LimitKind::Work))
    );
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
}

#[test]
fn attribute_cancellation_preserves_first_failure_and_retires_owners() {
    let early_work = accepted_attribute_work(&attribute_source(32, 12_000, false));
    let source = attribute_source(32, 12_000, true);
    for cancelled in [false, true] {
        for fail_cleanup in [false, true] {
            let (resources, account) = setup();
            account
                .reject_after
                .store(early_work + 1_000_000, Ordering::Relaxed);
            account.cancelled.store(cancelled, Ordering::Relaxed);
            account.fail_cleanup.store(fail_cleanup, Ordering::Relaxed);
            let result = parse(&source, &resources);
            assert!(result.is_err());
            let expected = if cancelled {
                Error::new(FailureKind::Cancelled, LimitKind::None)
            } else {
                Error::native(-19131)
            };
            assert_eq!(resources.failure(), Some(expected));
            drop(result);
            assert_eq!(resources.failure(), Some(expected));
            assert!(account.live.lock().unwrap().is_empty());
            assert_eq!(account.reserve_after_rejection.load(Ordering::Relaxed), 0);
            if fail_cleanup {
                assert!(resources.statistics().release_failures > 0);
                assert_eq!(resources.validate_retired(), Err(expected));
            } else {
                resources.validate_retired().unwrap();
            }
        }
    }
}

#[test]
fn attribute_expanded_names_keep_legacy_and_controlled_semantics() {
    let duplicate = "<e xmlns:a='urn:same' xmlns:b='urn:same' a:x='1' b:x='2'/>";
    assert!(matches!(
        Document::parse(duplicate),
        Err(roxmltree::Error::DuplicatedAttribute(_, _))
    ));
    let (resources, account) = setup();
    assert!(parse(duplicate, &resources).is_err());
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Decode, LimitKind::None))
    );
    assert!(account.live.lock().unwrap().is_empty());
    resources.validate_retired().unwrap();
    let source = "<e xmlns='urn:a' xmlns:a='urn:a' xmlns:b='urn:b' x='0' a:x='1' b:x='2'/>";
    for controlled in [false, true] {
        let (resources, account) = setup();
        let document = if controlled {
            parse(source, &resources).unwrap()
        } else {
            Document::parse(source).unwrap()
        };
        let element = document.root_element();
        assert_eq!(element.attribute("x"), Some("0"));
        assert_eq!(element.attribute(("urn:a", "x")), Some("1"));
        assert_eq!(element.attribute(("urn:b", "x")), Some("2"));
        drop(document);
        assert!(account.live.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
}
