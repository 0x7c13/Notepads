// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real filter-stage probes: build normalized XML before arming the backend.
use super::*;
use crate::tree::owner::TreeVec;
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

struct Probe {
    next: AtomicU64,
    live: AtomicU64,
    admitted: AtomicU64,
    released: AtomicU64,
    armed: AtomicBool,
    target_work: AtomicU64,
    hits: AtomicU64,
    failure: ResourceError,
    work: Mutex<Vec<u64>>,
}
impl Probe {
    fn arm(&self, target_work: u64) {
        self.target_work.store(target_work, Ordering::SeqCst);
        self.hits.store(0, Ordering::SeqCst);
        self.work.lock().unwrap().clear();
        self.armed.store(true, Ordering::SeqCst);
    }
}
struct ProbeBackend(Arc<Probe>);
impl Backend for ProbeBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
        let slot = self.0.next.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.live.fetch_add(bytes as u64, Ordering::SeqCst);
        Ticket::new(bytes as u64, slot as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), ResourceError> {
        assert_eq!(ticket.account_nonce(), bytes as u64);
        self.0.admitted.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
        let bytes = ticket.account_nonce();
        let previous = self.0.live.fetch_sub(bytes, Ordering::SeqCst);
        assert!(previous >= bytes);
        self.0.released.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), ResourceError> {
        if self.0.armed.load(Ordering::SeqCst) {
            self.0.work.lock().unwrap().push(work);
            if work == self.0.target_work.load(Ordering::SeqCst) {
                self.0.hits.fetch_add(1, Ordering::SeqCst);
                return Err(self.0.failure);
            }
        }
        Ok(())
    }
}
fn request(failure: ResourceError) -> (ResourcesRef, Arc<Probe>) {
    let probe = Arc::new(Probe {
        next: 0.into(),
        live: 0.into(),
        admitted: 0.into(),
        released: 0.into(),
        armed: false.into(),
        target_work: u64::MAX.into(),
        hits: 0.into(),
        failure,
        work: Mutex::new(Vec::new()),
    });
    let resources = ResourcesRef::new_prepaid(
        ProbeBackend(probe.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (resources, probe)
}
fn source(tag: &str, attribute: &str, value: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><filter id='f'><{tag} {attribute}='{value}'/></filter></svg>"
    )
}
fn retire(resources: ResourcesRef, probe: &Probe) {
    assert_eq!(probe.live.load(Ordering::SeqCst), 0);
    assert_eq!(
        probe.admitted.load(Ordering::SeqCst),
        probe.next.load(Ordering::SeqCst)
    );
    assert_eq!(
        probe.released.load(Ordering::SeqCst),
        probe.next.load(Ordering::SeqCst)
    );
    assert_eq!(resources.statistics().scratch_bytes, 0);
    assert_eq!(resources.statistics().live_leases, 0);
    resources.validate_retired().unwrap();
}
fn with_fe(
    source: &str,
    failure: ResourceError,
    test: impl FnOnce(SvgNode<'_, '_>, &ResourcesRef, &Probe),
) {
    let xml = roxmltree::Document::parse(source).unwrap();
    let (resources, probe) = request(failure);
    let doc = super::super::svgtree::Document::parse_tree_for_source(
        &xml,
        None,
        super::super::source_kind::SourceKind::GeneralImage,
        Some(resources.clone()),
    )
    .unwrap();
    let fe = doc
        .descendants()
        .find(|node| {
            matches!(
                node.tag_name(),
                Some(EId::FeGaussianBlur | EId::FeDropShadow | EId::FeConvolveMatrix)
            )
        })
        .unwrap();
    assert!(resources.failure().is_none());
    assert!(probe.live.load(Ordering::SeqCst) > 0);
    test(fe, &resources, &probe);
    drop(doc);
    retire(resources, &probe);
}
// Works against both the original Kind/tuple APIs and the later Result APIs.
// In particular, test-only contains no reference to the new svgtypes pull type.
fn filter_processing(node: SvgNode) -> crate::ProcessingContext {
    match node.resources() {
        Some(r) => crate::ProcessingContext::with_resources(r.clone()),
        None => crate::ProcessingContext::default(),
    }
}
fn exercise_actual_filter(fe: SvgNode) {
    let scale = Size::from_wh(1.0, 1.0).unwrap();
    match fe.tag_name().unwrap() {
        EId::FeGaussianBlur => {
            let _ = convert_gaussian_blur(fe, scale, &[], &filter_processing(fe));
        }
        EId::FeDropShadow => {
            let _ = convert_drop_shadow(fe, scale, &[], &filter_processing(fe));
        }
        EId::FeConvolveMatrix => {
            let _ = convert_convolve_matrix(fe, &[], &filter_processing(fe));
        }
        _ => unreachable!(),
    }
}
fn red_constructor(tag: &str) {
    let attribute = if tag == "feConvolveMatrix" {
        "order"
    } else {
        "stdDeviation"
    };
    let failure = ResourceError::native(-2701);
    // No kernelMatrix: its already-controlled parser must not explain this probe.
    with_fe(
        &source(tag, attribute, "1 2"),
        failure,
        |fe, resources, probe| {
            probe.arm(0);
            let before = resources.statistics().work;
            exercise_actual_filter(fe);
            eprintln!(
                "filter_numeric_constructor tag={tag} armed_work=0 hit={} work_delta={} first={:?}",
                probe.hits.load(Ordering::SeqCst),
                resources.statistics().work - before,
                resources.failure()
            );
            assert_eq!(
                resources.failure(),
                Some(failure),
                "actual {tag} bypassed zero-work request check"
            );
            assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
            assert_eq!(
                probe
                    .work
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|&&work| work == 0)
                    .count(),
                1
            );
        },
    );
}
fn red_scan(tag: &str, shape: usize) {
    let attribute = if tag == "feConvolveMatrix" {
        "order"
    } else {
        "stdDeviation"
    };
    let failure = ResourceError::new(FailureKind::Cancelled, LimitKind::Work);
    let value = match shape {
        0 => format!("{}1 1", "0".repeat(131073)),
        1 => format!("1 {}1", "0".repeat(131073)),
        2 => format!("1{}2", " ".repeat(131073)),
        _ => unreachable!(),
    };
    {
        with_fe(
            &source(tag, attribute, &value),
            failure,
            |fe, resources, probe| {
                let remaining = resources.scan_allowance().unwrap();
                resources.consume_scan_work(remaining).unwrap();
                probe.arm(65536);
                let before = resources.statistics().work;
                exercise_actual_filter(fe);
                eprintln!(
                    "filter_numeric_scan tag={tag} bytes={} armed_work=65536 hit={} work_delta={} first={:?}",
                    value.len(),
                    probe.hits.load(Ordering::SeqCst),
                    resources.statistics().work - before,
                    resources.failure()
                );
                assert_eq!(
                    resources.failure(),
                    Some(failure),
                    "actual {tag} bypassed numeric scan renewal"
                );
                assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
                assert_eq!(
                    probe
                        .work
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|&&work| work == 65536)
                        .count(),
                    1
                );
            },
        );
    }
}

#[test]
fn red_actual_gaussian_constructor_checks_request() {
    red_constructor("feGaussianBlur");
}
#[test]
fn red_actual_shadow_constructor_checks_request() {
    red_constructor("feDropShadow");
}
#[test]
fn red_actual_convolve_constructor_checks_request() {
    red_constructor("feConvolveMatrix");
}
#[test]
fn red_actual_gaussian_scan_first_token_checks_request() {
    red_scan("feGaussianBlur", 0);
}

#[test]
fn red_actual_gaussian_scan_second_token_checks_request() {
    red_scan("feGaussianBlur", 1);
}

#[test]
fn red_actual_gaussian_scan_separator_checks_request() {
    red_scan("feGaussianBlur", 2);
}

#[test]
fn red_actual_shadow_scan_first_token_checks_request() {
    red_scan("feDropShadow", 0);
}

#[test]
fn red_actual_shadow_scan_second_token_checks_request() {
    red_scan("feDropShadow", 1);
}

#[test]
fn red_actual_shadow_scan_separator_checks_request() {
    red_scan("feDropShadow", 2);
}

#[test]
fn red_actual_convolve_scan_first_token_checks_request() {
    red_scan("feConvolveMatrix", 0);
}

#[test]
fn red_actual_convolve_scan_second_token_checks_request() {
    red_scan("feConvolveMatrix", 1);
}

#[test]
fn red_actual_convolve_scan_separator_checks_request() {
    red_scan("feConvolveMatrix", 2);
}

fn with_plain_fe(source: &str, test: impl FnOnce(SvgNode<'_, '_>)) {
    let xml = roxmltree::Document::parse(source).unwrap();
    let doc = super::super::svgtree::Document::parse_tree_for_source(
        &xml,
        None,
        super::super::source_kind::SourceKind::GeneralImage,
        None,
    )
    .unwrap();
    let fe = doc
        .descendants()
        .find(|node| {
            matches!(
                node.tag_name(),
                Some(EId::FeGaussianBlur | EId::FeDropShadow | EId::FeConvolveMatrix)
            )
        })
        .unwrap();
    test(fe);
}
fn std_parameters(fe: SvgNode, scale: Size) -> (f32, f32) {
    match fe.tag_name().unwrap() {
        EId::FeGaussianBlur => {
            match convert_gaussian_blur(fe, scale, &[], &filter_processing(fe)).unwrap() {
                Kind::GaussianBlur(value) => (value.std_dev_x().get(), value.std_dev_y().get()),
                _ => unreachable!(),
            }
        }
        EId::FeDropShadow => {
            match convert_drop_shadow(fe, scale, &[], &filter_processing(fe)).unwrap() {
                Kind::DropShadow(value) => (value.std_dev_x().get(), value.std_dev_y().get()),
                _ => unreachable!(),
            }
        }
        _ => unreachable!(),
    }
}
#[test]
fn actual_stddev_parameters_keep_plain_recovery_scale_and_defaults() {
    let scale = Size::from_wh(2.0, 3.0).unwrap();
    for tag in ["feGaussianBlur", "feDropShadow"] {
        for (value, expected) in [
            ("1", (2.0, 3.0)),
            ("1 2", (2.0, 6.0)),
            ("1 2 bad", (2.0, 6.0)),
            ("1 bad 4", (2.0, 3.0)),
            ("1 2 3", (0.0, 0.0)),
            ("bad 2", (0.0, 0.0)),
            ("-1 2", (0.0, 6.0)),
            ("1e100 2", (0.0, 6.0)),
            ("1,", (2.0, 3.0)),
            ("", (0.0, 0.0)),
        ] {
            let source = source(tag, "stdDeviation", value);
            with_plain_fe(&source, |fe| {
                assert_eq!(std_parameters(fe, scale), expected)
            });
            with_fe(&source, ResourceError::native(-2701), |fe, resources, _| {
                assert_eq!(std_parameters(fe, scale), expected, "{tag}: {value}");
                assert!(resources.failure().is_none());
            });
        }
        let source =
            format!("<svg xmlns='http://www.w3.org/2000/svg'><filter><{tag}/></filter></svg>");
        let expected = if tag == "feGaussianBlur" {
            (0.0, 0.0)
        } else {
            (4.0, 6.0)
        };
        with_plain_fe(&source, |fe| {
            assert_eq!(std_parameters(fe, scale), expected)
        });
        with_fe(&source, ResourceError::native(-2701), |fe, _, _| {
            assert_eq!(std_parameters(fe, scale), expected);
        });
    }
}
fn convolve_dimensions(fe: SvgNode) -> (u32, u32) {
    match convert_convolve_matrix(fe, &[], &filter_processing(fe)).unwrap() {
        Kind::ConvolveMatrix(value) => (value.matrix().columns(), value.matrix().rows()),
        _ => unreachable!(),
    }
}
#[test]
fn actual_convolve_parameters_keep_plain_recovery_and_ignore_third_token() {
    for (order, expected) in [
        ("2 bad", (2, 2)),
        ("bad 2", (3, 3)),
        ("0 5", (3, 3)),
        ("2 1 ignored", (2, 1)),
        ("2 2 123", (2, 2)),
        ("1", (1, 1)),
        ("", (3, 3)),
    ] {
        let matrix = "1 ".repeat((expected.0 * expected.1) as usize);
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg'><filter><feConvolveMatrix order='{order}' kernelMatrix='{matrix}'/></filter></svg>"
        );
        with_plain_fe(&source, |fe| assert_eq!(convolve_dimensions(fe), expected));
        with_fe(&source, ResourceError::native(-2701), |fe, resources, _| {
            assert_eq!(convolve_dimensions(fe), expected, "{order}");
            assert!(resources.failure().is_none());
        });
    }
    let source = "<svg xmlns='http://www.w3.org/2000/svg'><filter><feConvolveMatrix kernelMatrix='1 1 1 1 1 1 1 1 1'/></filter></svg>";
    with_plain_fe(source, |fe| assert_eq!(convolve_dimensions(fe), (3, 3)));
    with_fe(source, ResourceError::native(-2701), |fe, _, _| {
        assert_eq!(convolve_dimensions(fe), (3, 3));
    });
}
fn assert_private_rejection(fe: SvgNode, expected: ResourceError) {
    let scale = Size::from_wh(1.0, 1.0).unwrap();
    match fe.tag_name().unwrap() {
        EId::FeGaussianBlur => {
            assert_eq!(
                convert_gaussian_blur(fe, scale, &[], &filter_processing(fe)).unwrap_err(),
                expected
            )
        }
        EId::FeDropShadow => assert_eq!(
            convert_drop_shadow(fe, scale, &[], &filter_processing(fe)).unwrap_err(),
            expected
        ),
        EId::FeConvolveMatrix => {
            assert!(convert_convolve_matrix(fe, &[], &filter_processing(fe)).is_none())
        }
        _ => unreachable!(),
    }
}
#[test]
fn actual_filter_control_preserves_typed_cancel_quota_native_and_existing_first() {
    for tag in ["feGaussianBlur", "feDropShadow", "feConvolveMatrix"] {
        let attribute = if tag == "feConvolveMatrix" {
            "order"
        } else {
            "stdDeviation"
        };
        for failure in [
            ResourceError::new(FailureKind::Cancelled, LimitKind::None),
            ResourceError::new(FailureKind::Quota, LimitKind::Work),
            ResourceError::native(-2703),
        ] {
            with_fe(
                &source(tag, attribute, "1 2"),
                failure,
                |fe, resources, probe| {
                    probe.arm(0);
                    assert_private_rejection(fe, failure);
                    assert_eq!(resources.failure(), Some(failure));
                    assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
                },
            );
        }
        let prior = ResourceError::native(-2704);
        with_fe(
            &source(tag, attribute, "1 2"),
            ResourceError::native(-2705),
            |fe, resources, probe| {
                resources.record(prior);
                probe.arm(0);
                assert_private_rejection(fe, prior);
                assert_eq!(resources.failure(), Some(prior));
                assert_eq!(probe.hits.load(Ordering::SeqCst), 0);
                assert!(probe.work.lock().unwrap().is_empty());
            },
        );
    }
}
fn leave_scan_credit(resources: &ResourcesRef, remaining: usize) {
    let current = resources.scan_allowance().unwrap();
    resources.consume_scan_work(current).unwrap();
    assert_eq!(resources.scan_allowance().unwrap(), 65536);
    resources.consume_scan_work(65536 - remaining).unwrap();
}
#[test]
fn actual_filter_renewal_after_two_windows_returns_original_native_failure() {
    let failure = ResourceError::native(-2706);
    for tag in ["feGaussianBlur", "feDropShadow", "feConvolveMatrix"] {
        let attribute = if tag == "feConvolveMatrix" {
            "order"
        } else {
            "stdDeviation"
        };
        let value = format!("{}1 1", "0".repeat(131073));
        with_fe(
            &source(tag, attribute, &value),
            failure,
            |fe, resources, probe| {
                leave_scan_credit(resources, 8192);
                probe.arm(65536);
                assert_private_rejection(fe, failure);
                assert_eq!(resources.failure(), Some(failure));
                assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
                let work = probe.work.lock().unwrap();
                assert_eq!(work.iter().filter(|&&work| work == 0).count(), 1);
                assert_eq!(work.iter().filter(|&&work| work == 65536).count(), 1);
            },
        );
    }
}
#[test]
fn actual_filter_ignored_tokens_stay_within_first_prepaid_window() {
    for (tag, value, expected, converted_token_bytes) in [
        (
            "feGaussianBlur",
            format!("1 2 bad{}", "9".repeat(131073)),
            (1.0, 2.0),
            2,
        ),
        (
            "feGaussianBlur",
            format!("1 bad 4{}", "9".repeat(131073)),
            (1.0, 1.0),
            1,
        ),
        (
            "feDropShadow",
            format!("1 2 3 {}", "9".repeat(131073)),
            (0.0, 0.0),
            3,
        ),
    ] {
        with_fe(
            &source(tag, "stdDeviation", &value),
            ResourceError::native(-2707),
            |fe, resources, probe| {
                leave_scan_credit(resources, 8192);
                probe.arm(65536);
                assert_eq!(
                    std_parameters(fe, Size::from_wh(1.0, 1.0).unwrap()),
                    expected
                );
                assert!(resources.failure().is_none());
                assert_eq!(probe.hits.load(Ordering::SeqCst), 0);
                // One original lexical window, plus the complete bytes of each
                // consumed f64 conversion token; ignored tails are still not read.
                assert_eq!(
                    resources.scan_allowance().unwrap(),
                    4096 - converted_token_bytes
                );
            },
        );
    }
    let value = format!("1 2 {}", "9".repeat(131073));
    with_fe(
        &source("feConvolveMatrix", "order", &value),
        ResourceError::native(-2707),
        |fe, resources, probe| {
            leave_scan_credit(resources, 8192);
            probe.arm(65536);
            // Missing kernel remains an ordinary None; the ignored third number
            // must not turn this into a quota/cancellation/parse request failure.
            assert!(convert_convolve_matrix(fe, &[], &filter_processing(fe)).is_none());
            assert!(resources.failure().is_none());
            assert_eq!(probe.hits.load(Ordering::SeqCst), 0);
            // order consumes exactly two one-byte tokens before missing kernel.
            assert_eq!(resources.scan_allowance().unwrap(), 4096 - 2);
        },
    );
}
#[test]
fn actual_filter_renewal_hits_the_real_resources_work_limit() {
    for tag in ["feGaussianBlur", "feDropShadow", "feConvolveMatrix"] {
        let attribute = if tag == "feConvolveMatrix" {
            "order"
        } else {
            "stdDeviation"
        };
        let value = format!("1 {}1", "0".repeat(131073));
        with_fe(
            &source(tag, attribute, &value),
            ResourceError::native(-2708),
            |fe, resources, probe| {
                let remaining = resources.scan_allowance().unwrap();
                resources.consume_scan_work(remaining).unwrap();
                let available = resources.limits().work - resources.statistics().work;
                resources.checkpoint(available - 65535).unwrap();
                probe.arm(u64::MAX);
                let failure = ResourceError::new(FailureKind::Quota, LimitKind::Work);
                assert_private_rejection(fe, failure);
                assert_eq!(resources.failure(), Some(failure));
                // Resources rejects before entering Backend's 65536 callback.
                assert!(!probe.work.lock().unwrap().contains(&65536));
            },
        );
    }
}
fn request_options(resources: &ResourcesRef) -> crate::Options<'static> {
    crate::Options {
        processing: Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
        ..Default::default()
    }
}
#[test]
fn actual_collect_children_discards_prefix_before_appending_failed_or_dummy_primitive() {
    let failure = ResourceError::native(-2709);
    for tag in ["feGaussianBlur", "feDropShadow", "feConvolveMatrix"] {
        let attribute = if tag == "feConvolveMatrix" {
            "order"
        } else {
            "stdDeviation"
        };
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg'><filter><feFlood/><{tag} {attribute}='1 2'/><feFlood/></filter></svg>"
        );
        with_fe(&source, failure, |fe, resources, probe| {
            let filter = fe
                .document()
                .descendants()
                .find(|n| n.tag_name() == Some(EId::Filter))
                .unwrap();
            let opt = request_options(resources);
            let region = NonZeroRect::from_xywh(0.0, 0.0, 8.0, 8.0).unwrap();
            let state = converter::State {
                refs: crate::parser::reference::RefPath::default(),
                context_proofs: [crate::parser::reference::PaintProof::default(); 2],
                parent_clip_path: None,
                parent_markers: TreeVec::new(&opt.processing),
                context_element: None,
                fe_image_link: false,
                view_box: region,
                use_size: (None, None),
                opt: &opt,
            };
            let mut cache = converter::Cache::with_processing(
                opt.processing.clone(),
                #[cfg(feature = "text")]
                opt.fontdb.clone(),
            );
            probe.arm(0);
            let primitives = collect_children(
                &filter,
                Units::UserSpaceOnUse,
                &state,
                None,
                region,
                &mut cache,
            );
            assert!(primitives.is_none());
            assert_eq!(resources.failure(), Some(failure));
            assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
        });
    }
}
#[test]
fn actual_tree_publication_gate_rejects_filter_control_then_fresh_request_recovers() {
    let failure = ResourceError::native(-2710);
    for tag in ["feGaussianBlur", "feDropShadow", "feConvolveMatrix"] {
        let attribute = if tag == "feConvolveMatrix" {
            "order"
        } else {
            "stdDeviation"
        };
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><defs><filter id='f'><{tag} {attribute}='1 2'/></filter></defs><rect width='8' height='8' filter='url(#f)'/></svg>"
        );
        let xml = roxmltree::Document::parse(&source).unwrap();
        let (resources, probe) = request(failure);
        let opt = request_options(&resources);
        let doc = super::super::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            super::super::source_kind::SourceKind::GeneralImage,
            Some(resources.clone()),
        )
        .unwrap();
        assert!(resources.failure().is_none());
        probe.arm(0);
        let result = converter::convert_doc(&doc, &opt);
        drop(doc);
        // This is the same existing final cleanup/publication check used by
        // from_xmltree_in_request, after the actual normalized owner retires.
        let result = crate::Tree::after_owned_cleanup(result, &opt);
        assert!(result.is_err());
        assert_eq!(resources.failure(), Some(failure));
        assert_eq!(probe.hits.load(Ordering::SeqCst), 1);
        drop(result);
        drop(opt);
        retire(resources, &probe);

        let (resources, probe) = request(failure);
        let opt = request_options(&resources);
        let result = crate::Tree::from_data_controlled(source.as_bytes(), &opt).unwrap();
        assert!(!result.root().children().is_empty());
        assert!(resources.failure().is_none());
        drop(result);
        drop(opt);
        retire(resources, &probe);
    }
}
