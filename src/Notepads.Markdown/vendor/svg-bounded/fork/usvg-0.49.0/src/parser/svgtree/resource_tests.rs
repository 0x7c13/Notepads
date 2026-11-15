// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

struct Ledger {
    next: AtomicU64,
    live: AtomicU64,
    peak: AtomicU64,
    calls: AtomicU64,
    cancel_at: AtomicU64,
    cancelled_with_live: AtomicU64,
    releases: AtomicU64,
    fail_release: AtomicU64,
    injected_release: AtomicU64,
    failed_bytes: AtomicU64,
    release_history: Mutex<Vec<(u64, usize)>>,
    role_releases: Mutex<Vec<(u8, u64, u64, usize)>>,
    entries: Mutex<Vec<(u64, usize)>>,
}
struct TestBackend(Arc<Ledger>);
impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
        let id = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        let live = self.0.live.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
        self.0.peak.fetch_max(live, Ordering::Relaxed);
        self.0.entries.lock().unwrap().push((id, bytes));
        Ticket::new(1, id as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
        let mut entries = self.0.entries.lock().unwrap();
        let i = entries
            .iter()
            .position(|x| x.0 == u64::from(ticket.slot()))
            .unwrap();
        let (_, bytes) = entries[i];
        self.0
            .release_history
            .lock()
            .unwrap()
            .push((u64::from(ticket.slot()), bytes));
        let n = self.0.releases.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.role_releases.lock().unwrap().push((
            crate::parser::cleanup_phase(),
            n,
            u64::from(ticket.slot()),
            bytes,
        ));
        if self.0.fail_release.load(Ordering::Relaxed) == n {
            self.0.injected_release.store(n, Ordering::Relaxed);
            self.0.failed_bytes.store(bytes as u64, Ordering::Relaxed);
            return Err(ResourceError::native(-2147467259));
        }
        entries.swap_remove(i);
        self.0.live.fetch_sub(bytes as u64, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
        let n = self.0.calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.0.cancel_at.load(Ordering::Relaxed) == n {
            self.0
                .cancelled_with_live
                .store(self.0.live.load(Ordering::Relaxed), Ordering::Relaxed);
            return Err(ResourceError::new(
                FailureKind::Cancelled,
                markdown_svg_budget::LimitKind::Work,
            ));
        }
        Ok(())
    }
}
fn setup(limit: u64) -> (ResourcesRef, Arc<Ledger>) {
    setup_limits(Limits {
        scratch_bytes: limit,
        ..Limits::default()
    })
}
fn setup_limits(limits: Limits) -> (ResourcesRef, Arc<Ledger>) {
    let state = Arc::new(Ledger {
        next: 0.into(),
        live: 0.into(),
        peak: 0.into(),
        calls: 0.into(),
        cancel_at: 0.into(),
        cancelled_with_live: 0.into(),
        releases: 0.into(),
        fail_release: 0.into(),
        injected_release: 0.into(),
        failed_bytes: 0.into(),
        release_history: Mutex::new(Vec::with_capacity(4096)),
        role_releases: Mutex::new(Vec::new()),
        entries: Mutex::new(Vec::with_capacity(4096)),
    });
    let resources = ResourcesRef::new_prepaid(
        TestBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        limits,
    );
    (resources, state)
}
fn normalize<'a>(
    xml: &roxmltree::Document<'a>,
    resources: Option<ResourcesRef>,
) -> Result<Document<'a>, roxmltree::Error> {
    Document::parse_tree_for_source(
        xml,
        None,
        crate::parser::source_kind::SourceKind::GeneralImage,
        resources,
    )
}
fn retire(resources: ResourcesRef, ledger: &Ledger) {
    assert_eq!(ledger.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn duplicate_xml_first_and_normalized_last_id_semantics_are_distinct() {
    let xml=roxmltree::Document::parse(r##"<svg xmlns="http://www.w3.org/2000/svg"><g id="same"><rect/></g><g id="same"><circle/></g><use href="#same"/><text id="t">first</text><text id="t">last</text><text><tref href="#t"/></text></svg>"##).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let doc = normalize(&xml, Some(resources.clone())).unwrap();
    assert_eq!(
        doc.element_by_id("same")
            .unwrap()
            .first_element_child()
            .unwrap()
            .tag_name(),
        Some(EId::Circle)
    );
    let used = doc
        .descendants()
        .find(|n| n.tag_name() == Some(EId::Use))
        .unwrap();
    assert_eq!(
        used.first_element_child()
            .unwrap()
            .first_element_child()
            .unwrap()
            .tag_name(),
        Some(EId::Rect)
    );
    let texts: Vec<_> = doc
        .descendants()
        .filter(|n| n.is_text())
        .map(|n| n.text())
        .collect();
    assert_eq!(texts, ["first", "last", "first"]);
    drop(doc);
    retire(resources, &ledger);
}
#[test]
fn normalized_last_id_uses_descendant_order_not_node_allocation_order() {
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let mut doc = Document {
        nodes: Storage::new(Some(&resources)),
        attrs: Storage::new(Some(&resources)),
        links: Storage::new(Some(&resources)),
        work: Work::new(Some(resources.clone())),
        font_document: false,
        expand_uses: true,
        selection_wrapper: None,
    };
    doc.nodes
        .try_push(NodeData {
            source_id: 0,
            text_reference: u32::MAX,
            parent: None,
            next_sibling: None,
            children: None,
            kind: NodeKind::Root,
        })
        .unwrap();
    let a = doc
        .append(
            NodeId::new(0),
            NodeKind::Element {
                tag_name: EId::G,
                attributes: ShortRange::new(0, 0),
            },
        )
        .unwrap();
    doc.attrs
        .try_push(Attribute {
            name: AId::Id,
            value: roxmltree::StringStorage::Borrowed("duplicate"),
            important: false,
        })
        .unwrap();
    let b = doc
        .append(
            NodeId::new(0),
            NodeKind::Element {
                tag_name: EId::Circle,
                attributes: ShortRange::new(0, 1),
            },
        )
        .unwrap();
    doc.attrs
        .try_push(Attribute {
            name: AId::Id,
            value: roxmltree::StringStorage::Borrowed("duplicate"),
            important: false,
        })
        .unwrap();
    let child = doc
        .append(
            a,
            NodeKind::Element {
                tag_name: EId::Rect,
                attributes: ShortRange::new(1, 2),
            },
        )
        .unwrap();
    assert!(child.get_usize() > b.get_usize());
    doc.rebuild_links().unwrap();
    assert_eq!(doc.element_by_id("duplicate").unwrap().id, b);
    drop(doc);
    retire(resources, &ledger);
}
#[test]
fn text_whitespace_unicode_tref_and_cr_keep_compatibility_semantics() {
    let xml=roxmltree::Document::parse("<svg xmlns='http://www.w3.org/2000/svg'><text> 中\t <tspan>Á 👩‍💻</tspan>  后 </text><text xml:space='preserve'> a\t b </text></svg>").unwrap();
    let legacy = normalize(&xml, None).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let paid = normalize(&xml, Some(resources.clone())).unwrap();
    let a: Vec<_> = legacy
        .descendants()
        .filter(|n| n.is_text())
        .map(|n| n.text())
        .collect();
    let b: Vec<_> = paid
        .descendants()
        .filter(|n| n.is_text())
        .map(|n| n.text())
        .collect();
    assert_eq!(a, b);
    drop(paid);
    retire(resources, &ledger);
}
#[test]
fn full_normalizer_live_peak_minus_equal_plus_and_fresh_recovery() {
    let xml=roxmltree::Document::parse("<svg xmlns='http://www.w3.org/2000/svg'><g id='x' style='fill:red;stroke:blue'><text> a  b </text></g><use href='#x'/></svg>").unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    drop(normalize(&xml, Some(resources.clone())).unwrap());
    let peak = ledger.peak.load(Ordering::Relaxed);
    retire(resources, &ledger);
    for limit in [peak - 1, peak, peak + 1] {
        let (resources, ledger) = setup(limit);
        let result = normalize(&xml, Some(resources.clone()));
        assert_eq!(result.is_ok(), limit >= peak);
        drop(result);
        retire(resources, &ledger);
    }
    let (resources, ledger) = setup(64 * 1024 * 1024);
    drop(normalize(&xml, Some(resources.clone())).unwrap());
    retire(resources, &ledger);
}
#[test]
fn long_id_comparison_cancels_with_live_index_and_fresh_scope_recovers() {
    let prefix = "x".repeat(90000);
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg'><g id='{prefix}a'/><g id='{prefix}b'/><g id='{prefix}c'/></svg>"
    );
    let xml = roxmltree::Document::parse(&source).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    ledger.cancel_at.store(3, Ordering::Relaxed);
    let result = normalize(&xml, Some(resources.clone()));
    assert!(result.is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
    assert!(ledger.cancelled_with_live.load(Ordering::Relaxed) > 0);
    drop(result);
    retire(resources, &ledger);
    let (resources, ledger) = setup(64 * 1024 * 1024);
    drop(normalize(&xml, Some(resources.clone())).unwrap());
    retire(resources, &ledger);
}
#[test]
fn optional_id_lookup_records_real_mid_comparison_cancel_and_tree_cannot_publish() {
    let id = "x".repeat(90000);
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect id='{id}'/></svg>"
    );
    let xml = roxmltree::Document::parse(&source).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let doc = normalize(&xml, Some(resources.clone())).unwrap();
    let opt = request_options(&resources);
    let calls = ledger.calls.load(Ordering::Relaxed);
    ledger.cancel_at.store(calls + 1, Ordering::Relaxed);
    assert!(resources.failure().is_none());
    assert!(doc.element_by_id(&id).is_none());
    assert!(doc.work.checked().is_err());
    assert!(ledger.calls.load(Ordering::Relaxed) > calls);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
    assert!(ledger.cancelled_with_live.load(Ordering::Relaxed) > 0);
    let result = crate::Tree::from_xmltree_in_request(&xml, &opt);
    assert!(
        matches!(result,Err(crate::Error::ProcessingFailure(e)) if e.typed_failure.kind==FailureKind::Cancelled)
    );
    drop(result);
    drop(doc);
    drop(opt);
    retire(resources, &ledger);
}
#[test]
fn fallible_heapsort_preserves_equal_key_ordinals_and_stops_on_comparator_error() {
    let mut values = [(2, 2), (1, 4), (2, 0), (1, 1)];
    storage::try_sort(&mut values, |a, b| Ok(a.cmp(b))).unwrap();
    assert_eq!(values, [(1, 1), (1, 4), (2, 0), (2, 2)]);
    let mut values = [9, 8, 7, 6];
    let mut comparisons = 0;
    let err = storage::try_sort(&mut values, |a, b| {
        comparisons += 1;
        if comparisons == 2 {
            Err(roxmltree::Error::NoRootNode)
        } else {
            Ok(a.cmp(b))
        }
    });
    assert!(err.is_err());
    assert_eq!(comparisons, 2);
}

fn request_options(resources: &ResourcesRef) -> crate::Options<'static> {
    crate::Options {
        processing: Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
        ..Default::default()
    }
}
fn retire_after_injected_release_failure(resources: ResourcesRef, ledger: &Ledger) {
    let bytes = ledger.failed_bytes.load(Ordering::Relaxed);
    assert!(bytes > 0);
    assert_eq!(ledger.live.load(Ordering::Relaxed), bytes);
    assert_eq!(ledger.entries.lock().unwrap().len(), 1);
    let statistics = resources.statistics();
    assert_eq!(statistics.release_failures, 1);
    assert_eq!(statistics.live_leases, 1);
    assert_eq!(statistics.scratch_bytes, bytes);
    assert_eq!(statistics.depth, 0);
    assert_eq!(
        resources.validate_retired().unwrap_err(),
        resources.failure().unwrap()
    );
}
fn assert_native_cleanup_failure(
    result: &Result<crate::Tree, crate::Error>,
    resources: &ResourcesRef,
) {
    let expected = ResourceError::native(-2147467259);
    assert_eq!(resources.failure(), Some(expected));
    assert!(matches!(result,Err(crate::Error::ProcessingFailure(e)) if e.typed_failure==expected));
}
const EMPTY_IMAGE: &str =
    "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><g id='x'/></svg>";
#[test]
fn caller_owned_paid_xml_survives_normalizer_last_release_failure_and_tree_is_not_published() {
    let mut point = None;
    for inject in [false, true] {
        let (resources, ledger) = setup(64 * 1024 * 1024);
        let opt = request_options(&resources);
        let xml = roxmltree::Document::parse_with_options(
            EMPTY_IMAGE,
            roxmltree::ParsingOptions {
                resources: Some(resources.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let xml_live = ledger.live.load(Ordering::Relaxed);
        assert!(xml_live > 0);
        let before = ledger.releases.load(Ordering::Relaxed);
        if inject {
            ledger.fail_release.store(point.unwrap(), Ordering::Relaxed);
        }
        let result = crate::Tree::from_xmltree_in_request(&xml, &opt);
        let after = ledger.releases.load(Ordering::Relaxed);
        assert!(after > before);
        if inject {
            assert_eq!(
                ledger.live.load(Ordering::Relaxed),
                xml_live + ledger.failed_bytes.load(Ordering::Relaxed)
            );
        } else {
            assert!(ledger.live.load(Ordering::Relaxed) > xml_live);
        }

        assert_eq!(xml.root_element().attribute("width"), Some("8"));
        if inject {
            assert_eq!(
                ledger.injected_release.load(Ordering::Relaxed),
                point.unwrap()
            );
            assert_native_cleanup_failure(&result, &resources);
        } else {
            assert!(result.is_ok());
            let events = ledger.role_releases.lock().unwrap();
            let (_, ordinal, _, _) = events
                .iter()
                .rev()
                .find(|e| e.0 == 2)
                .copied()
                .expect("actual normalizer cleanup event");
            point = Some(ordinal);
        }
        drop(result);
        assert_eq!(
            ledger.live.load(Ordering::Relaxed),
            xml_live + ledger.failed_bytes.load(Ordering::Relaxed)
        );
        drop(xml);
        drop(opt);
        if inject {
            retire_after_injected_release_failure(resources, &ledger);
        } else {
            retire(resources, &ledger);
        }
    }
}
#[test]
fn owned_paid_xml_last_release_failure_cannot_publish_success() {
    let mut point = None;
    let mut final_ticket = None;
    for inject in [false, true] {
        let (resources, ledger) = setup(64 * 1024 * 1024);
        let opt = request_options(&resources);
        if inject {
            ledger.fail_release.store(point.unwrap(), Ordering::Relaxed);
        }
        let result = crate::Tree::from_str_in_request(EMPTY_IMAGE, &opt);
        if inject {
            assert_eq!(
                ledger.live.load(Ordering::Relaxed),
                ledger.failed_bytes.load(Ordering::Relaxed)
            );
        } else {
            assert!(ledger.live.load(Ordering::Relaxed) > 0);
        }
        let (_, ordinal, id, bytes) = ledger
            .role_releases
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|e| e.0 == 1)
            .copied()
            .expect("actual owned XML cleanup event");
        let last = (id, bytes);
        if inject {
            assert_eq!(Some(last), final_ticket);
            assert_eq!(
                ledger.injected_release.load(Ordering::Relaxed),
                point.unwrap()
            );
            assert_native_cleanup_failure(&result, &resources);
        } else {
            assert!(result.is_ok());
            point = Some(ordinal);
            final_ticket = Some(last);
        }
        drop(result);
        assert_eq!(
            ledger.live.load(Ordering::Relaxed),
            ledger.failed_bytes.load(Ordering::Relaxed)
        );
        drop(opt);
        if inject {
            retire_after_injected_release_failure(resources, &ledger);
        } else {
            retire(resources, &ledger);
        }
    }
}
#[cfg(feature = "font-svg-gzip")]
#[test]
fn font_gzip_owned_decoded_buffer_last_release_failure_cannot_publish_success() {
    use std::io::Write;
    let source = "<svg xmlns='http://www.w3.org/2000/svg'><g id='glyph1'/></svg>";
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(source.as_bytes()).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut point = None;
    let mut final_ticket = None;
    for inject in [false, true] {
        let (resources, ledger) = setup(64 * 1024 * 1024);
        let mut opt = request_options(&resources);
        opt.source_kind = crate::parser::source_kind::SourceKind::FontGlyph {
            glyph: 1,
            units_per_em: 1000,
            current_color: svgtypes::Color::black(),
        };
        if inject {
            ledger.fail_release.store(point.unwrap(), Ordering::Relaxed);
        }
        let result = crate::Tree::from_data_in_request(&compressed, &opt);
        if inject {
            assert_eq!(
                ledger.live.load(Ordering::Relaxed),
                ledger.failed_bytes.load(Ordering::Relaxed)
            );
        } else {
            assert!(ledger.live.load(Ordering::Relaxed) > 0);
        }
        let (_, ordinal, id, bytes) = ledger
            .role_releases
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|e| e.0 == 3)
            .copied()
            .expect("actual owned decoded-buffer cleanup");
        let last = (id, bytes);
        if inject {
            assert_eq!(Some(last), final_ticket);
            assert_eq!(
                ledger.injected_release.load(Ordering::Relaxed),
                point.unwrap()
            );
            assert_native_cleanup_failure(&result, &resources);
        } else {
            assert!(result.is_ok(), "{result:?}");
            point = Some(ordinal);
            final_ticket = Some(last);
            assert!(last.1 >= source.len());
        }
        drop(result);
        assert_eq!(
            ledger.live.load(Ordering::Relaxed),
            ledger.failed_bytes.load(Ordering::Relaxed)
        );
        drop(opt);
        if inject {
            retire_after_injected_release_failure(resources, &ledger);
        } else {
            retire(resources, &ledger);
        }
    }
}
#[test]
fn earlier_conversion_error_keeps_its_variant_and_first_record_across_cleanup_hr() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='0' height='0'/>";
    let mut point = None;
    for inject in [false, true] {
        let (resources, ledger) = setup(64 * 1024 * 1024);
        let opt = request_options(&resources);
        if inject {
            ledger.fail_release.store(point.unwrap(), Ordering::Relaxed);
        }
        let result = crate::Tree::from_str_in_request(source, &opt);
        assert!(matches!(result, Err(crate::Error::InvalidSize)));
        assert_eq!(
            resources.failure(),
            Some(ResourceError::new(
                FailureKind::Decode,
                markdown_svg_budget::LimitKind::None
            ))
        );
        if inject {
            assert_eq!(
                ledger.injected_release.load(Ordering::Relaxed),
                point.unwrap()
            );
        } else {
            point = Some(ledger.releases.load(Ordering::Relaxed));
            assert!(point.unwrap() > 0);
        }
        drop(result);
        drop(opt);
        if inject {
            retire_after_injected_release_failure(resources, &ledger);
        } else {
            retire(resources, &ledger);
        }
    }
}

#[test]
fn structural_root_access_does_not_panic_when_next_iterator_checkpoint_would_cancel() {
    let xml = roxmltree::Document::parse(EMPTY_IMAGE).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let doc = normalize(&xml, Some(resources.clone())).unwrap();
    let previous = ledger.calls.load(Ordering::Relaxed);
    loop {
        doc.work.event().unwrap();
        if ledger.calls.load(Ordering::Relaxed) > previous {
            break;
        }
    }
    for _ in 0..255 {
        doc.work.event().unwrap();
    }
    ledger
        .cancel_at
        .store(ledger.calls.load(Ordering::Relaxed) + 1, Ordering::Relaxed);
    assert_eq!(doc.root_element().tag_name(), Some(EId::Svg));
    assert!(resources.failure().is_none());
    assert!(doc.root().first_element_child().is_none());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
    assert_eq!(doc.root_element().tag_name(), Some(EId::Svg));
    assert_eq!(
        doc.root_element()
            .first_child()
            .unwrap()
            .parent_element()
            .unwrap()
            .tag_name(),
        Some(EId::Svg)
    );
    assert!(doc.root_element().parent_element().is_none());
    drop(doc);
    retire(resources, &ledger);
}

#[test]
fn deep_use_expansion_and_large_unicode_text_cancel_mid_work_then_fresh_scope_recovers() {
    let mut nested = String::from("<svg xmlns='http://www.w3.org/2000/svg'><g id='n0'><rect/></g>");
    for i in 1..36 {
        nested.push_str(&format!("<g id='n{i}'><use href='#n{}'/></g>", i - 1));
    }
    nested.push_str("<use href='#n35'/></svg>");
    let large = format!(
        "<svg xmlns='http://www.w3.org/2000/svg'><text>{}</text></svg>",
        "中 Á 👩‍💻  ".repeat(24000)
    );
    for (source, large_text) in [(&nested, false), (&large, true)] {
        let xml = roxmltree::Document::parse(source).unwrap();
        let (resources, ledger) = setup(64 * 1024 * 1024);
        let doc = normalize(&xml, Some(resources.clone())).unwrap();
        if large_text {
            assert!(
                doc.descendants()
                    .find(|n| n.is_text())
                    .unwrap()
                    .text()
                    .len()
                    > 300000
            );
        } else {
            assert!(doc.nodes.len() > 1200);
        }
        let calls = ledger.calls.load(Ordering::Relaxed);
        assert!(calls > 8);
        drop(doc);
        retire(resources, &ledger);
        let (resources, ledger) = setup(64 * 1024 * 1024);
        ledger.cancel_at.store(calls / 2, Ordering::Relaxed);
        let result = normalize(&xml, Some(resources.clone()));
        assert!(result.is_err());
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
        assert!(
            ledger.cancelled_with_live.load(Ordering::Relaxed)
                > if large_text { 65536 } else { 8192 }
        );
        drop(result);
        retire(resources, &ledger);
        let (resources, ledger) = setup(64 * 1024 * 1024);
        drop(normalize(&xml, Some(resources.clone())).unwrap());
        retire(resources, &ledger);
    }
}
#[test]
fn empty_namespaced_ids_use_marker_clip_and_paid_entity_clones_keep_semantics() {
    use markdown_svg_budget::PaidShared;
    let source = "<svg xmlns='http://www.w3.org/2000/svg' xmlns:z='urn:test'><g id=''/><g z:id='only-namespaced'/><g id='item&#49;'><rect/></g><use href='#item1'/><marker id='m'><path/></marker><clipPath id='c'><rect/></clipPath><rect marker-end='url(#m)' clip-path='url(#c)'/></svg>";
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let xml = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            resources: Some(resources.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let raw = xml
        .descendants()
        .find(|n| n.attribute("id") == Some("item1"))
        .unwrap();
    let raw_value = raw.attributes().find(|a| a.name() == "id").unwrap();
    let roxmltree::StringStorage::Paid(original) = raw_value.value_storage() else {
        panic!("entity must be paid owned XML text")
    };
    let doc = normalize(&xml, Some(resources.clone())).unwrap();
    assert_eq!(doc.element_by_id("").unwrap().tag_name(), Some(EId::G));
    assert!(doc.element_by_id("only-namespaced").is_none());
    let node = doc.element_by_id("item1").unwrap();
    let slot = node.attribute_id(AId::Id).unwrap();
    let roxmltree::StringStorage::Paid(cloned) = &doc.attrs[slot].value else {
        panic!("normalized attr must retain paid XML owner")
    };
    assert!(PaidShared::ptr_eq(original, cloned));
    assert_eq!(
        doc.element_by_id("m").unwrap().tag_name(),
        Some(EId::Marker)
    );
    assert_eq!(
        doc.element_by_id("c").unwrap().tag_name(),
        Some(EId::ClipPath)
    );
    let rect = doc
        .descendants()
        .filter(|n| n.tag_name() == Some(EId::Rect))
        .last()
        .unwrap();
    assert_eq!(
        rect.node_attribute(AId::MarkerEnd).unwrap().element_id(),
        "m"
    );
    assert_eq!(
        rect.node_attribute(AId::ClipPath).unwrap().element_id(),
        "c"
    );
    drop(doc);
    assert_eq!(raw_value.value(), "item1");
    assert!(ledger.live.load(Ordering::Relaxed) > 0);
    drop(xml);
    retire(resources, &ledger);
}
#[test]
fn semantic_normalizer_error_is_recorded_before_cleanup_hresult() {
    let xml = roxmltree::Document::parse("<g xmlns='http://www.w3.org/2000/svg'/>").unwrap();
    let mut point = None;
    for inject in [false, true] {
        let (resources, ledger) = setup(64 * 1024 * 1024);
        if inject {
            ledger.fail_release.store(point.unwrap(), Ordering::Relaxed);
        }
        let result = normalize(&xml, Some(resources.clone()));
        assert!(matches!(result, Err(roxmltree::Error::NoRootNode)));
        assert_eq!(
            resources.failure(),
            Some(ResourceError::new(
                FailureKind::Decode,
                markdown_svg_budget::LimitKind::None
            ))
        );
        if inject {
            assert_eq!(
                ledger.injected_release.load(Ordering::Relaxed),
                point.unwrap()
            );
            drop(result);
            retire_after_injected_release_failure(resources, &ledger);
        } else {
            point = Some(ledger.releases.load(Ordering::Relaxed));
            assert!(point.unwrap() > 0);
            drop(result);
            retire(resources, &ledger);
        }
    }
}
#[test]
fn invisible_root_real_converter_ok_is_rejected_after_normalizer_cleanup_hresult() {
    let source = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8' display='none'><g id='x'/></svg>";
    let xml = roxmltree::Document::parse(source).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let opt = request_options(&resources);
    crate::parser::source_kind::validate(&xml, &opt).unwrap();
    let doc = normalize(&xml, Some(resources.clone())).unwrap();
    let result = crate::parser::converter::convert_doc(&doc, &opt);
    assert!(result.is_ok());
    assert!(result.as_ref().unwrap().root().children().is_empty());
    assert!(resources.failure().is_none());
    drop(doc);
    let point = ledger.releases.load(Ordering::Relaxed);
    assert!(point > 0);
    drop(result);
    drop(opt);
    retire(resources, &ledger);
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let opt = request_options(&resources);
    ledger.fail_release.store(point, Ordering::Relaxed);
    let result = crate::Tree::from_xmltree_in_request(&xml, &opt);
    assert_native_cleanup_failure(&result, &resources);
    assert_eq!(ledger.injected_release.load(Ordering::Relaxed), point);
    drop(result);
    drop(opt);
    retire_after_injected_release_failure(resources, &ledger);
}

#[test]
fn expanded_use_nodes_keep_existing_policy_and_real_scratch_work_limits() {
    let mut source = String::from(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><g id='many'>",
    );
    for _ in 0..400 {
        source.push_str("<g/>");
    }
    source.push_str("</g></defs>");
    for _ in 0..200 {
        source.push_str("<use href='#many'/>");
    }
    source.push_str("</svg>");
    assert_eq!(source.len(), 5497);
    let xml = roxmltree::Document::parse(&source).unwrap();
    assert_eq!(xml.descendants().filter(|n| n.is_element()).count(), 603);
    let legacy = normalize(&xml, None).unwrap();
    assert!(legacy.nodes.len() > 80000);
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let paid = normalize(&xml, Some(resources.clone())).unwrap();
    assert_eq!(paid.nodes.len(), legacy.nodes.len());
    assert_eq!(paid.attrs.len(), legacy.attrs.len());
    assert_eq!(
        paid.descendants()
            .filter_map(|n| n.tag_name())
            .collect::<Vec<_>>(),
        legacy
            .descendants()
            .filter_map(|n| n.tag_name())
            .collect::<Vec<_>>()
    );
    drop(paid);
    drop(legacy);
    retire(resources, &ledger);
    let legacy = crate::Tree::from_data(source.as_bytes(), &crate::Options::default()).unwrap();
    let (resources, ledger) = setup(64 * 1024 * 1024);
    let opt = request_options(&resources);
    let paid = crate::Tree::from_data_controlled(source.as_bytes(), &opt).unwrap();
    assert_eq!(legacy.size(), paid.size());
    assert_eq!(legacy.root().children().len(), paid.root().children().len());
    drop(paid);
    drop(opt);
    retire(resources, &ledger);
    for limits in [
        Limits {
            scratch_bytes: 1024 * 1024,
            ..Limits::default()
        },
        Limits {
            scratch_bytes: 64 * 1024 * 1024,
            work: 250000,
            ..Limits::default()
        },
    ] {
        let expected = if limits.scratch_bytes == 1024 * 1024 {
            markdown_svg_budget::LimitKind::Scratch
        } else {
            markdown_svg_budget::LimitKind::Work
        };
        let (resources, ledger) = setup_limits(limits);
        let opt = request_options(&resources);
        let result = crate::Tree::from_data_controlled(source.as_bytes(), &opt);
        assert!(result.is_err());
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Quota);
        assert_eq!(resources.failure().unwrap().limit, expected);
        assert!(ledger.peak.load(Ordering::Relaxed) > 65536);
        drop(result);
        drop(opt);
        retire(resources, &ledger);
    }
}
