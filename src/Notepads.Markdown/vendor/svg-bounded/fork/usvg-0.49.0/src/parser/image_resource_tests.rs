// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, Limits, ResourceKind,
    ResourcesRef, Ticket,
};
use std::fmt::Write;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Mutex,
};

struct State {
    next: AtomicU64,
    used: AtomicUsize,
    reserves: AtomicUsize,
    releases: AtomicUsize,
    late: AtomicBool,
    arm_after_release_bytes: AtomicUsize,
}
#[derive(Clone)]
struct Ledger(Arc<State>);
impl Backend for Ledger {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
        self.0.used.fetch_add(bytes, Ordering::SeqCst);
        self.0.reserves.fetch_add(1, Ordering::SeqCst);
        let id = self.0.next.fetch_add(1, Ordering::SeqCst);
        Ticket::new(bytes as u64, id as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
        self.0
            .used
            .fetch_sub(ticket.account_nonce() as usize, Ordering::SeqCst);
        let late = self.0.late.load(Ordering::SeqCst);
        self.0.releases.fetch_add(1, Ordering::SeqCst);
        if ticket.account_nonce() as usize == self.0.arm_after_release_bytes.load(Ordering::SeqCst)
        {
            self.0.late.store(true, Ordering::SeqCst);
        }
        if late {
            Err(ResourceError::native(-777))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
        Ok(())
    }
}
fn request() -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        next: AtomicU64::new(1),
        used: AtomicUsize::new(0),
        reserves: AtomicUsize::new(0),
        releases: AtomicUsize::new(0),
        late: AtomicBool::new(false),
        arm_after_release_bytes: AtomicUsize::new(usize::MAX),
    });
    (
        ResourcesRef::new_prepaid(
            Ledger(state.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        state,
    )
}
fn options(resources: &ResourcesRef) -> Options<'static> {
    let mut opt = Options::default();
    opt.processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
    opt
}
fn url(mime: &str, bytes: &[u8]) -> String {
    let mut result = format!("data:{mime},");
    for byte in bytes {
        write!(&mut result, "%{byte:02X}").unwrap();
    }
    result
}
fn document(href: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><image href='{href}' width='4' height='4'/></svg>"
    )
}
fn first_png(group: &Group) -> Option<&crate::EncodedImageData> {
    group.children().iter().find_map(|node| match node {
        Node::Group(group) => first_png(group),
        Node::Image(image) => match image.kind() {
            ImageKind::PNG(data) => Some(data),
            ImageKind::SVG(tree) => first_png(tree.root()),
            _ => None,
        },
        _ => None,
    })
}
// One opaque red RGBA pixel, generated from PNG signature/IHDR/zlib/IEND with
// standard zlib+CRC outside the engine; no platform font or user asset bytes.
const PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
fn real_data_image_keeps_one_admitted_owner_after_tree_retirement() {
    let (resources, state) = request();
    let opt = options(&resources);
    let tree =
        Tree::from_data_controlled(document(&url("image/png", PNG)).as_bytes(), &opt).unwrap();
    assert_eq!(resources.statistics().children, 1);
    assert_eq!(resources.statistics().child_encoded_bytes, PNG.len() as u64);
    let data = first_png(tree.root()).unwrap();
    assert!(data.is_admitted_for(&resources));
    assert_eq!(data.as_slice(), PNG);
    let before = state.reserves.load(Ordering::SeqCst);
    let escaped = data.try_alias_in(&opt.processing).unwrap();
    assert_eq!(state.reserves.load(Ordering::SeqCst), before);
    drop(tree);
    drop(opt);
    assert!(state.used.load(Ordering::SeqCst) > 0);
    drop(escaped);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn nested_svg_bytes_and_children_are_admitted_once_and_raster_has_no_svg_depth() {
    let inner = document(&url("image/png", PNG));
    let (resources, state) = request();
    let opt = options(&resources);
    let tree = Tree::from_data_controlled(
        document(&url("image/svg+xml", inner.as_bytes())).as_bytes(),
        &opt,
    )
    .unwrap();
    assert!(first_png(tree.root()).unwrap().is_admitted_for(&resources));
    assert_eq!(resources.statistics().children, 2);
    assert_eq!(
        resources.statistics().child_encoded_bytes,
        (inner.len() + PNG.len()) as u64
    );
    assert_eq!(resources.statistics().depth, 0);
    drop(tree);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn custom_callback_runs_once_with_paid_essence_and_escaped_clone_keeps_funding() {
    let (resources, state) = request();
    let mut opt = options(&resources);
    let escaped = Arc::new(Mutex::new(None));
    let captured = escaped.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    opt.image_href_resolver.resolve_data = crate::ImageHrefDataResolverFn::Owned(Box::new(move |mime, data, config| {
        assert_eq!(mime, "image/png");
        assert!(data.is_admitted_for(config.processing.resources().unwrap()));
        observed.fetch_add(1, Ordering::SeqCst);
        *captured.lock().unwrap() = Some(data.try_alias_in(&config.processing).unwrap());
        Some(ImageKind::PNG(data))
    }));
    let tree =
        Tree::from_data_controlled(document(&url("image/png", PNG)).as_bytes(), &opt).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(tree);
    drop(opt);
    assert!(state.used.load(Ordering::SeqCst) > 0);
    drop(escaped.lock().unwrap().take());
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn callback_plain_and_other_request_raster_outputs_are_rejected() {
    let (resources, state) = request();
    let mut opt = options(&resources);
    let plain: crate::EncodedImageData = Arc::new(PNG.to_vec()).into();
    let plain = Mutex::new(Some(plain));
    opt.image_href_resolver.resolve_data =
        crate::ImageHrefDataResolverFn::Owned(Box::new(move |_, _, _| plain.lock().unwrap().take().map(ImageKind::PNG)));
    assert!(Tree::from_data_controlled(document(&url("image/png", PNG)).as_bytes(), &opt).is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
    let (resources, state) = request();
    let (other, other_state) = request();
    let mut opt = options(&resources);
    let foreign = crate::EncodedImageData::try_copy_from_slice(&other, PNG).unwrap();
    let foreign = Mutex::new(Some(foreign));
    opt.image_href_resolver.resolve_data =
        crate::ImageHrefDataResolverFn::Owned(Box::new(move |_, _, _| foreign.lock().unwrap().take().map(ImageKind::PNG)));
    assert!(Tree::from_data_controlled(document(&url("image/png", PNG)).as_bytes(), &opt).is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
    assert_eq!(other.failure().unwrap().kind, FailureKind::Invariant);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert_eq!(other_state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
    other.validate_retired().unwrap();
}

#[test]
fn callback_svg_must_keep_same_processing_arc_and_general_image_origin() {
    for wrong_origin in [false, true] {
        let (resources, state) = request();
        let mut opt = options(&resources);
        opt.image_href_resolver.resolve_data = crate::ImageHrefDataResolverFn::Owned(Box::new(move |_, _, config| {
            let mut nested = Options::default();
            nested.processing = if wrong_origin {
                config.processing.clone()
            } else {
                Arc::new(crate::ProcessingContext::with_resources(
                    config.processing.resources().unwrap().clone(),
                ))
            };
            if wrong_origin {
                nested.source_kind = super::super::source_kind::SourceKind::GeneratedColorGlyph;
            }
            let tree=Tree::from_data_controlled(b"<svg xmlns='http://www.w3.org/2000/svg' width='1' height='1'><rect width='1' height='1'/></svg>",&nested).unwrap();
            if wrong_origin {
                assert!(Arc::ptr_eq(tree.processing_context(), &config.processing));
            }
            Some(ImageKind::SVG(tree))
        }));
        assert!(
            Tree::from_data_controlled(document(&url("image/png", PNG)).as_bytes(), &opt).is_err()
        );
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
        drop(opt);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn callback_none_is_latched_before_its_last_known_input_can_release() {
    let (resources, state) = request();
    let mut opt = options(&resources);
    let arm = state.clone();
    opt.image_href_resolver.resolve_data = crate::ImageHrefDataResolverFn::Owned(Box::new(move |_, data, _| {
        assert_eq!(data.as_slice(), PNG);
        arm.late.store(true, Ordering::SeqCst);
        None
    }));
    assert!(get_href_data_controlled(&url("image/png", PNG), &opt, &resources).is_none());
    assert_eq!(
        resources.failure().unwrap().kind,
        FailureKind::UnsupportedImageData
    );
    assert_eq!(resources.failure().unwrap().native_hr, None);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert!(resources.statistics().release_failures > 0);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn valid_callback_is_not_published_after_mime_header_cleanup_hresult() {
    let (resources, state) = request();
    let mut opt = options(&resources);
    let arm = state.clone();
    opt.image_href_resolver.resolve_data = crate::ImageHrefDataResolverFn::Owned(Box::new(move |_, data, _| {
        arm.late.store(true, Ordering::SeqCst);
        Some(ImageKind::PNG(data))
    }));
    assert!(get_href_data_controlled(&url("image/png", PNG), &opt, &resources).is_none());
    assert_eq!(resources.failure(), Some(ResourceError::native(-777)));
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn fatal_base64_is_latched_while_its_paid_header_is_still_live() {
    let (resources, state) = request();
    let opt = options(&resources);
    // The temporary AVL now retires before the normalized header buffer. Arm
    // only after that exact 19B normalized header has successfully retired,
    // while the parsed MIME fields still live and the body is not yet read.
    assert_eq!("text/plain;x=header".len(), 19);
    state.arm_after_release_bytes.store(19, Ordering::SeqCst);
    assert!(
        get_href_data_controlled("data:text/plain;x=header;base64,!!!!", &opt, &resources)
            .is_none()
    );
    assert_eq!(
        resources.failure().unwrap().kind,
        FailureKind::MalformedImageData
    );
    assert_eq!(resources.failure().unwrap().native_hr, None);
    drop(opt);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert!(resources.statistics().release_failures > 0);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn header_syntax_keeps_the_original_lowercase_prefix_and_string_fallback() {
    let (resources, _) = request();
    let mut opt = options(&resources);
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let replacement = crate::EncodedImageData::try_copy_from_slice(&resources, PNG).unwrap();
    opt.image_href_resolver.resolve_string = crate::ImageHrefStringResolverFn::Owned(Box::new(move |_, config| {
        count.fetch_add(1, Ordering::SeqCst);
        Some(ImageKind::PNG(
            replacement.try_alias_in(&config.processing).unwrap(),
        ))
    }));
    assert!(get_href_data_controlled("DATA:no-comma", &opt, &resources).is_some());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(get_href_data_controlled("data:no-comma", &opt, &resources).is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        resources.failure().unwrap().kind,
        FailureKind::MalformedImageData
    );
    drop(opt);
    resources.validate_retired().unwrap();
}

#[test]
fn real_svg_data_children_hit_64_and_reject_65_without_publishing_a_tree() {
    let image = format!(
        "<image href='{}' width='1' height='1'/>",
        url("image/png", PNG)
    );
    for count in [64, 65] {
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'>{}</svg>",
            image.repeat(count)
        );
        let (resources, state) = request();
        let opt = options(&resources);
        let result = Tree::from_data_controlled(source.as_bytes(), &opt);
        assert_eq!(result.is_ok(), count == 64);
        if count == 64 {
            assert_eq!(resources.statistics().children, 64);
            assert_eq!(
                resources.statistics().child_encoded_bytes,
                64 * PNG.len() as u64
            );
            assert_eq!(resources.statistics().depth, 0);
        } else {
            assert_eq!(
                resources.failure().unwrap().limit,
                markdown_svg_budget::LimitKind::Children
            );
        }
        drop(result);
        drop(opt);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn real_svg_decoded_children_hit_eight_mib_minus_equal_plus_once() {
    use base64::Engine;
    let mut large = PNG.to_vec();
    // The PNG profile explicitly accepts opaque bytes after IEND. Two uses of
    // one 4MiB-ish source keep the top-level XML below its separate input cap.
    let small_middle = PNG.len() + 2;
    large.resize(((8 << 20) - small_middle) / 2, b'a');
    let large_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&large)
    );
    for delta in [-1isize, 0, 1] {
        let mut small = PNG.to_vec();
        small.resize((small_middle as isize + delta) as usize, b'b');
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'><defs><image id='p' href='{large_url}' width='1' height='1'/></defs><use href='#p'/><use href='#p'/><image href='{}' width='1' height='1'/></svg>",
            url("image/png", &small)
        );
        assert!(source.len() < 8 << 20);
        let (resources, state) = request();
        let opt = options(&resources);
        let result = Tree::from_data_controlled(source.as_bytes(), &opt);
        assert_eq!(
            result.is_ok(),
            delta <= 0,
            "aggregate delta {delta}, failure {:?}",
            resources.failure()
        );
        if delta <= 0 {
            assert_eq!(resources.statistics().children, 3);
            assert_eq!(
                resources.statistics().child_encoded_bytes,
                ((8 << 20) as isize + delta) as u64
            );
        } else {
            assert_eq!(
                resources.failure().unwrap().limit,
                markdown_svg_budget::LimitKind::ChildEncoded
            );
        }
        drop(result);
        drop(opt);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn real_svg_depth_eight_allows_terminal_raster_and_depth_nine_is_rejected() {
    use base64::Engine;
    for depth in [8, 9] {
        let mut source = document(&url("image/png", PNG));
        let mut expected_encoded = PNG.len();
        for _ in 0..depth {
            expected_encoded += source.len();
            source = document(&format!(
                "data:image/svg+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(source.as_bytes())
            ));
        }
        assert!(source.len() < 8 << 20);
        let (resources, state) = request();
        let opt = options(&resources);
        let result = Tree::from_data_controlled(source.as_bytes(), &opt);
        assert_eq!(
            result.is_ok(),
            depth == 8,
            "depth {depth}, failure {:?}",
            resources.failure()
        );
        if depth == 8 {
            assert!(first_png(result.as_ref().unwrap().root())
                .unwrap()
                .is_admitted_for(&resources));
            assert_eq!(resources.statistics().children, 9);
            assert_eq!(
                resources.statistics().child_encoded_bytes,
                expected_encoded as u64
            );
            assert_eq!(resources.statistics().depth, 0);
        } else {
            assert_eq!(
                resources.failure().unwrap().limit,
                markdown_svg_budget::LimitKind::SvgDepth
            );
        }
        drop(result);
        drop(opt);
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
        resources.validate_retired().unwrap();
    }
}
