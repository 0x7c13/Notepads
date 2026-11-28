// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::reference_cases::{self as fixtures, Expected};
use super::*;
use markdown_svg_budget::{
    Backend, Error as Fault, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[derive(Default)]
struct Ledger {
    used: AtomicUsize,
    next: AtomicUsize,
    fail_release: AtomicUsize,
    fault: std::sync::OnceLock<FirstFailureRef>,
}
struct BackendOwner(Arc<Ledger>);
impl Backend for BackendOwner {
    fn reserve(&self, _: ResourceKind, n: usize) -> Result<Ticket, Fault> {
        self.0.used.fetch_add(n, Ordering::SeqCst);
        Ticket::new(
            n as u64,
            self.0.next.fetch_add(1, Ordering::SeqCst) as u32 + 1,
            1,
        )
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Fault> {
        Ok(())
    }
    fn release_after_free(&self, t: Ticket) -> Result<(), Fault> {
        if self.0.fault.get().and_then(FirstFailureRef::get).is_some()
            && self.0.fail_release.swap(0, Ordering::SeqCst) != 0
        {
            return Err(Fault::native(-9064));
        }
        self.0
            .used
            .fetch_sub(t.account_nonce() as usize, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Fault> {
        Ok(())
    }
}
fn trial(source: &str, expected: Expected, font: bool, prior: bool, release: bool) {
    let ledger = Arc::new(Ledger::default());
    let r = ResourcesRef::new_prepaid(
        BackendOwner(ledger.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    ledger.fault.set(r.fault_slot().clone()).unwrap();
    let p = Arc::new(crate::ProcessingContext::with_resources(r.clone()));
    let mut o = Options {
        processing: p.clone(),
        ..Options::default()
    };
    if font {
        o.source_kind = source_kind::SourceKind::FontGlyph {
            glyph: 1,
            units_per_em: 32,
            current_color: svgtypes::Color::black(),
        };
    }
    if prior {
        r.record(Fault::native(-9063));
    }
    if release {
        ledger.fail_release.store(1, Ordering::SeqCst);
    }
    let tree = crate::Tree::from_data_in_request(source.as_bytes(), &o);
    if prior {
        assert_eq!(r.failure(), Some(Fault::native(-9063)));
        assert!(tree.is_err());
    } else {
        match expected {
            Expected::Accept => {
                assert!(tree.is_ok(), "{tree:?}");
                assert!(tree.as_ref().unwrap().root().has_children());
            }
            Expected::Quota => {
                assert!(tree.is_err(), "{tree:?}");
                assert_eq!(
                    r.failure(),
                    Some(Fault::new(FailureKind::Quota, LimitKind::Capacity))
                );
            }
            Expected::Decode => {
                assert!(tree.is_err(), "{tree:?}");
                assert_eq!(
                    r.failure(),
                    Some(Fault::new(FailureKind::Decode, LimitKind::None))
                );
            }
        }
    }
    assert_eq!((p.counts().2, p.counts().3), (0, 0));
    drop(tree);
    drop(o);
    drop(p);
    if release {
        assert!(r.validate_retired().is_err());
        assert!(ledger.used.load(Ordering::SeqCst) > 0);
    } else {
        r.validate_retired().unwrap();
        assert_eq!(ledger.used.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn reference_guard_actual_six_groups() {
    for case in fixtures::cases() {
        println!("REFERENCE_CASE {} {:?}", case.name, case.expected);
        trial(&case.source, case.expected, false, false, false);
        if case.plain {
            assert!(
                crate::Tree::from_data(case.source.as_bytes(), &Options::default()).is_ok(),
                "plain {}",
                case.name
            );
        }
    }
}
#[test]
fn reference_guard_selected_glyph_is_zero_edge_and_unselected_sibling_is_not_expanded() {
    for edges in [64, 65] {
        let source = fixtures::document(&format!(
            "<g id='glyph1'>{}</g><g id='glyph2'><use href='#glyph2'/></g>",
            fixtures::use_body(edges, "<rect width='20' height='20'/>", "")
        ));
        trial(
            &source,
            if edges == 64 {
                Expected::Accept
            } else {
                Expected::Quota
            },
            true,
            false,
            false,
        );
    }
}
#[test]
fn reference_guard_first_fault_and_release_tail_keep_canonical_error() {
    let source = fixtures::document(&fixtures::use_body(
        65,
        "<rect width='20' height='20'/>",
        "",
    ));
    trial(&source, Expected::Quota, false, true, false);
    trial(&source, Expected::Quota, false, false, true);
}
#[test]
fn reference_guard_inline_sizes_are_actual() {
    println!(
        "REFERENCE_SIZES path={} frames={} state={} cache={}",
        std::mem::size_of::<reference::RefPath>(),
        std::mem::size_of::<reference::RefFrames>(),
        std::mem::size_of::<converter::State>(),
        std::mem::size_of::<converter::Cache>()
    );
    assert_eq!(std::mem::size_of::<reference::RefPath>(), 264);
}

#[test]
fn reference_review_materialized_colors_have_no_context_uri_edge() {
    for (stops, expected) in [
        ("<stop stop-color='red'/>", Expected::Accept),
        ("", Expected::Accept),
        (
            "<stop stop-color='red'/><stop offset='1' stop-color='blue'/>",
            Expected::Quota,
        ),
    ] {
        let source = fixtures::document(&format!(
            "<defs><linearGradient id='p'>{stops}</linearGradient><rect id='leaf' width='20' height='20' fill='context-fill'/></defs>{}",
            fixtures::use_body(63, "<use href='#leaf' fill='url(#p) red'/>", "")
        ));
        trial(&source, expected, false, false, false);
        assert!(crate::Tree::from_data(source.as_bytes(), &Options::default()).is_ok());
    }
}

#[test]
fn reference_review_foreign_parent_fault_precedes_new_copy_checkpoint() {
    review_foreign_input(true);
    review_foreign_input(false);
}

fn review_foreign_input(foreign_parent: bool) {
    use crate::geometry_test_support::{request, Call};
    let source =
        fixtures::document("<clipPath id='clip'><rect width='20' height='20'/></clipPath>");
    let xml = roxmltree::Document::parse(&source).unwrap();
    let (source_r, _) = request(Limits::default());
    let doc = svgtree::Document::parse_tree_for_source(
        &xml,
        None,
        source_kind::SourceKind::GeneralImage,
        Some(source_r.clone()),
    )
    .unwrap();
    let (target_r, target_ledger) = request(Limits::default());
    let options = Options {
        processing: Arc::new(crate::ProcessingContext::with_resources(target_r.clone())),
        ..Options::default()
    };
    let source_processing = crate::ProcessingContext::with_resources(source_r.clone());
    let markers_processing = if foreign_parent {
        options.processing.as_ref()
    } else {
        &source_processing
    };
    let state = converter::State {
        refs: reference::RefPath::default(),
        context_proofs: Default::default(),
        parent_clip_path: foreign_parent.then(|| doc.root_element()),
        parent_markers: crate::tree::owner::TreeVec::new(markers_processing),
        context_element: None,
        fe_image_link: false,
        view_box: crate::NonZeroRect::from_xywh(0.0, 0.0, 32.0, 32.0).unwrap(),
        use_size: (None, None),
        opt: &options,
    };
    let original = Fault::native(-9764);
    source_r.record(original);
    target_ledger.arm(Call::Checkpoint, 1, Fault::native(-9765));
    assert_eq!(state.try_clone().err(), Some(original));
    assert_eq!(source_r.failure(), Some(original));
    assert_eq!(target_r.failure(), Some(original));
    assert_eq!(target_ledger.counts().checkpoint, 0);
    drop(state);
    drop(options);
    drop(doc);
    drop(source_processing);
    source_r.validate_retired().unwrap();
    target_r.validate_retired().unwrap();
}
