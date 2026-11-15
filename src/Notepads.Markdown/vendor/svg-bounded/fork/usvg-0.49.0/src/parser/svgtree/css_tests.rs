// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::super::{AId, Document, EId};
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
#[derive(Default)]
struct State {
    next: AtomicU64,
    live: AtomicU64,
    releases: AtomicU64,
    fail: AtomicU64,
    failed_bytes: AtomicU64,
    calls: AtomicU64,
    cancel: AtomicU64,
    entries: Mutex<Vec<(u64, usize)>>,
}
struct TestBackend(Arc<State>);
impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, n: usize) -> Result<Ticket, ResourceError> {
        let id = self.0.next.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.entries.lock().unwrap().push((id, n));
        self.0.live.fetch_add(n as u64, Ordering::SeqCst);
        Ticket::new(1, id as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
        Ok(())
    }
    fn release_after_free(&self, t: Ticket) -> Result<(), ResourceError> {
        let mut entries = self.0.entries.lock().unwrap();
        let i = entries.iter().position(|x| x.0 == u64::from(t.slot())).unwrap();
        let n = entries[i].1;
        let call = self.0.releases.fetch_add(1, Ordering::SeqCst) + 1;
        if self.0.fail.load(Ordering::SeqCst) == call {
            self.0.failed_bytes.store(n as u64, Ordering::SeqCst);
            return Err(ResourceError::native(-827));
        }
        entries.swap_remove(i);
        self.0.live.fetch_sub(n as u64, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
        let call = self.0.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.0.cancel.load(Ordering::SeqCst) == call {
            Err(ResourceError::new(
                FailureKind::Cancelled,
                markdown_svg_budget::LimitKind::Work,
            ))
        } else {
            Ok(())
        }
    }
}
fn request() -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State::default());
    let r = ResourcesRef::new_prepaid(
        TestBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (r, state)
}
fn normalize<'a>(
    xml: &roxmltree::Document<'a>,
    r: Option<ResourcesRef>,
) -> Result<Document<'a>, roxmltree::Error> {
    Document::parse_tree_for_source(
        xml,
        None,
        crate::parser::source_kind::SourceKind::GeneralImage,
        r,
    )
}
fn rows(doc: &Document<'_>) -> Vec<(Option<EId>, Vec<(AId, String, bool)>)> {
    doc.descendants()
        .map(|n| {
            (
                n.tag_name(),
                n.attributes()
                    .iter()
                    .map(|a| (a.name, a.value.as_str().to_owned(), a.important))
                    .collect(),
            )
        })
        .collect()
}
#[test]
fn controlled_css_normalization_matches_original_cascade_and_use_text() {
    let source = r##"<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><style>.a{fill:red}g .a{stroke:green!important} #same{opacity:.5}</style><style>.a{fill:blue} .a:first-child{stroke:yellow!important}</style><defs><g id='same' class='a'><rect width='10' height='10'/></g></defs><use href='#same'/><g><g class='a' style='fill:#123;marker:url(#m)'/><text class='a'><tspan style='font:italic 12px Arial'>中文🙂</tspan></text></g></svg>"##;
    let xml = roxmltree::Document::parse(source).unwrap();
    let legacy = normalize(&xml, None).unwrap();
    let (r, state) = request();
    let paid = normalize(&xml, Some(r.clone())).unwrap();
    assert_eq!(rows(&paid), rows(&legacy));
    assert!(state.next.load(Ordering::SeqCst) > 0);
    drop(paid);
    r.validate_retired().unwrap();
    assert_eq!(state.live.load(Ordering::SeqCst), 0);
}
#[test]
fn css_sheet_cleanup_native_error_is_seen_before_normalized_document_success() {
    let plain_xml = roxmltree::Document::parse(
        "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><g/></svg>",
    )
    .unwrap();
    let (r, state) = request();
    state.fail.store(1, Ordering::SeqCst);
    let no_sheet = normalize(&plain_xml, Some(r.clone())).unwrap();
    assert_eq!(state.releases.load(Ordering::SeqCst), 0);
    state.fail.store(0, Ordering::SeqCst);
    drop(no_sheet);
    r.validate_retired().unwrap();
    let xml=roxmltree::Document::parse("<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><style>g{fill:red}</style><g/></svg>").unwrap();
    let (r, state) = request();
    state.fail.store(1, Ordering::SeqCst);
    let error = normalize(&xml, Some(r.clone()))
        .err()
        .expect("paid CSS cleanup must reject publication");
    assert!(matches!(error,roxmltree::Error::Resource(e) if e==ResourceError::native(-827)));
    assert_eq!(r.failure(), Some(ResourceError::native(-827)));
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(
        state.live.load(Ordering::SeqCst),
        state.failed_bytes.load(Ordering::SeqCst)
    );
    assert!(state.live.load(Ordering::SeqCst) > 0);
    assert!(r.validate_retired().is_err());
}
#[test]
fn every_actual_css_normalizer_checkpoint_cancels_without_panic_or_publication() {
    let source=format!("<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><style>{}g .a{{fill:red;stroke:blue}} .a > g + g:first-child{{opacity:.5}}</style><g><g class='a'/><g class='a'/></g></svg>","/*中文🙂*/".repeat(200));
    let xml = roxmltree::Document::parse(&source).unwrap();
    let (r, state) = request();
    let doc = normalize(&xml, Some(r.clone())).unwrap();
    let calls = state.calls.load(Ordering::SeqCst);
    drop(doc);
    r.validate_retired().unwrap();
    assert!(calls > 0);
    for cancel in 1..=calls {
        let (r, state) = request();
        state.cancel.store(cancel, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            normalize(&xml, Some(r.clone()))
        }));
        let result = result.expect("controlled CSS cancellation must not panic");
        let error = result
            .err()
            .expect("cancelled normalization must not succeed");
        assert!(
            matches!(error,roxmltree::Error::Resource(e) if e.kind==FailureKind::Cancelled),
            "checkpoint {cancel}: {error:?}"
        );
        r.validate_retired().unwrap();
        assert_eq!(state.live.load(Ordering::SeqCst), 0);
    }
}
