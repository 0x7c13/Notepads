// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
mod oracle {
    use super::*;
    pub(super) fn payload_bytes(payload: &LocalPayload) -> Option<usize> {
        match payload {
            LocalPayload::Ordinary(Some(path)) => path_bytes(path),
            // This Arc references the already admitted Native geometry owner.
            LocalPayload::Ordinary(None) | LocalPayload::Synthetic { .. } => Some(0),
            LocalPayload::Bitmap(image) => image_bytes(&image.image),
            LocalPayload::Colr(group) => group_bytes(group, 0),
            LocalPayload::Svg(node) => node_bytes(node, 0),
        }
    }

    pub(super) fn group_bytes(group: &Group, depth: usize) -> Option<usize> {
        if depth > 256 {
            return None;
        }
        let mut bytes = size_of::<Group>()
            .checked_add(group.id.capacity())?
            .checked_add(group.children.capacity().checked_mul(size_of::<Node>())?)?
            .checked_add(
                group
                    .filters
                    .capacity()
                    .checked_mul(size_of::<Arc<crate::filter::Filter>>())?,
            )?;
        for node in &group.children {
            bytes = bytes.checked_add(node_bytes(node, depth + 1)?)?;
        }
        if let Some(clip) = &group.clip_path {
            bytes = bytes.checked_add(clip_bytes(clip, depth + 1)?)?;
        }
        if let Some(mask) = &group.mask {
            bytes = bytes.checked_add(mask_bytes(mask, depth + 1)?)?;
        }
        for filter in &group.filters {
            bytes = bytes.checked_add(filter_bytes(filter, depth + 1)?)?;
        }
        Some(bytes)
    }

    pub(super) fn node_bytes(node: &Node, depth: usize) -> Option<usize> {
        if depth > 256 {
            return None;
        }
        let bytes = match node {
            Node::Group(group) => group_bytes(group, depth + 1)?,
            Node::Path(path) => size_of::<crate::Path>()
                .checked_add(path.id.capacity())?
                .checked_add(path_owner_bytes(&path.data)?)?
                .checked_add(
                    path.stroke
                        .as_ref()
                        .and_then(|s| s.dasharray.as_ref())
                        .map_or(Some(0), |v| {
                            v.retained_unpaid_capacity().checked_mul(size_of::<f32>())
                        })?,
                )?
                .checked_add(
                    path.fill
                        .as_ref()
                        .map_or(Some(0), |fill| paint_bytes(&fill.paint, depth + 1))?,
                )?
                .checked_add(
                    path.stroke
                        .as_ref()
                        .map_or(Some(0), |stroke| paint_bytes(&stroke.paint, depth + 1))?,
                )?,
            Node::Image(image) => image_bytes(image)?,
            // Nested font SVG text retains another checked plan and scene. Its
            // shared-owner admission is a separate mandatory gate, never guessed.
            Node::Text(_) => return None,
        };
        // Referenced resources are counted per reference, including their dynamic
        // arrays. This may overcharge sharing, but never treats an Arc as its data.
        Some(bytes)
    }

    pub(super) fn id_bytes(id: &str) -> Option<usize> {
        id.len().checked_mul(2)?.checked_add(8)
    }

    pub(super) fn paint_bytes(paint: &crate::Paint, depth: usize) -> Option<usize> {
        match paint {
            crate::Paint::Color(_) => Some(0),
            crate::Paint::LinearGradient(gradient) => size_of::<crate::LinearGradient>()
                .checked_add(id_bytes(gradient.id())?)?
                .checked_add(
                    gradient
                        .stops
                        .capacity()
                        .checked_mul(size_of::<crate::Stop>())?,
                ),
            crate::Paint::RadialGradient(gradient) => size_of::<crate::RadialGradient>()
                .checked_add(id_bytes(gradient.id())?)?
                .checked_add(
                    gradient
                        .stops
                        .capacity()
                        .checked_mul(size_of::<crate::Stop>())?,
                ),
            crate::Paint::Pattern(pattern) => size_of::<crate::Pattern>()
                .checked_add(id_bytes(pattern.id())?)?
                .checked_add(group_bytes(pattern.root(), depth + 1)?),
        }
    }

    pub(super) fn clip_bytes(clip: &crate::ClipPath, depth: usize) -> Option<usize> {
        if depth > 256 {
            return None;
        }
        let bytes = size_of::<crate::ClipPath>()
            .checked_add(id_bytes(clip.id())?)?
            .checked_add(group_bytes(clip.root(), depth + 1)?)?;
        bytes.checked_add(
            clip.clip_path()
                .map_or(Some(0), |clip| clip_bytes(clip, depth + 1))?,
        )
    }

    pub(super) fn mask_bytes(mask: &crate::Mask, depth: usize) -> Option<usize> {
        if depth > 256 {
            return None;
        }
        let bytes = size_of::<crate::Mask>()
            .checked_add(id_bytes(mask.id())?)?
            .checked_add(group_bytes(mask.root(), depth + 1)?)?;
        bytes.checked_add(
            mask.mask()
                .map_or(Some(0), |mask| mask_bytes(mask, depth + 1))?,
        )
    }

    pub(super) fn input_bytes(input: &crate::filter::Input) -> usize {
        match input {
            crate::filter::Input::Reference(value) => value.capacity(),
            _ => 0,
        }
    }

    pub(super) fn transfer_bytes(value: &crate::filter::TransferFunction) -> Option<usize> {
        use crate::filter::TransferFunction;
        match value {
            TransferFunction::Table(values) | TransferFunction::Discrete(values) => values
                .retained_unpaid_capacity()
                .checked_mul(size_of::<f32>()),
            _ => Some(0),
        }
    }

    pub(super) fn filter_bytes(filter: &crate::filter::Filter, depth: usize) -> Option<usize> {
        use crate::filter::{ColorMatrixKind, Kind};
        if depth > 256 {
            return None;
        }
        let mut bytes = size_of::<crate::filter::Filter>()
            .checked_add(id_bytes(filter.id())?)?
            .checked_add(
                filter
                    .primitives
                    .capacity()
                    .checked_mul(size_of::<crate::filter::Primitive>())?,
            )?;
        for primitive in &filter.primitives {
            bytes = bytes.checked_add(primitive.result.capacity())?;
            let extra = match &primitive.kind {
                Kind::Blend(value) => {
                    input_bytes(&value.input1).checked_add(input_bytes(&value.input2))?
                }
                Kind::Composite(value) => {
                    input_bytes(&value.input1).checked_add(input_bytes(&value.input2))?
                }
                Kind::DisplacementMap(value) => {
                    input_bytes(&value.input1).checked_add(input_bytes(&value.input2))?
                }
                Kind::ColorMatrix(value) => {
                    input_bytes(&value.input).checked_add(match &value.kind {
                        ColorMatrixKind::Matrix(values) => values
                            .retained_unpaid_capacity()
                            .checked_mul(size_of::<f32>())?,
                        _ => 0,
                    })?
                }
                Kind::ComponentTransfer(value) => input_bytes(&value.input)
                    .checked_add(transfer_bytes(&value.func_r)?)?
                    .checked_add(transfer_bytes(&value.func_g)?)?
                    .checked_add(transfer_bytes(&value.func_b)?)?
                    .checked_add(transfer_bytes(&value.func_a)?)?,
                Kind::ConvolveMatrix(value) => input_bytes(&value.input).checked_add(
                    value
                        .matrix
                        .data
                        .retained_unpaid_capacity()
                        .checked_mul(size_of::<f32>())?,
                )?,
                Kind::DiffuseLighting(value) => input_bytes(&value.input),
                Kind::DropShadow(value) => input_bytes(&value.input),
                Kind::GaussianBlur(value) => input_bytes(&value.input),
                Kind::Image(value) => group_bytes(value.root(), depth + 1)?,
                Kind::Merge(value) => {
                    let mut sum = value
                        .inputs
                        .capacity()
                        .checked_mul(size_of::<crate::filter::Input>())?;
                    for input in &value.inputs {
                        sum = sum.checked_add(input_bytes(input))?;
                    }
                    sum
                }
                Kind::Morphology(value) => input_bytes(&value.input),
                Kind::Offset(value) => input_bytes(&value.input),
                Kind::SpecularLighting(value) => input_bytes(&value.input),
                Kind::Tile(value) => input_bytes(&value.input),
                Kind::Flood(_) | Kind::Turbulence(_) => 0,
            };
            bytes = bytes.checked_add(extra)?;
        }
        Some(bytes)
    }

    pub(super) fn image_bytes(image: &crate::Image) -> Option<usize> {
        let data_bytes = match image.kind() {
            crate::ImageKind::JPEG(data)
            | crate::ImageKind::PNG(data)
            | crate::ImageKind::GIF(data)
            | crate::ImageKind::WEBP(data) => data.retained_unpaid_capacity(),
            crate::ImageKind::SVG(tree) => {
                let vectors = tree
                    .linear_gradients
                    .capacity()
                    .checked_add(tree.radial_gradients.capacity())?
                    .checked_add(tree.patterns.capacity())?
                    .checked_add(tree.clip_paths.capacity())?
                    .checked_add(tree.masks.capacity())?
                    .checked_add(tree.filters.capacity())?;
                size_of::<crate::Tree>()
                    .checked_add(group_bytes(tree.root(), 0)?)?
                    .checked_add(vectors.checked_mul(size_of::<Arc<crate::Group>>())?)?
            }
        };
        size_of::<crate::Image>()
            .checked_add(image.id.capacity())?
            .checked_add(data_bytes)
    }
}

fn plain_tree(source: &str) -> crate::Tree {
    crate::Tree::from_str(source, &crate::Options::default()).unwrap()
}
fn scene_source() -> String {
    let mut source=r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><defs>
    <rect id="inner" width="2" height="3"/>
    <linearGradient id="linear"><stop offset="0"/><stop offset="1"/></linearGradient>
    <radialGradient id="radial"><stop offset="0"/><stop offset="1"/></radialGradient>
    <pattern id="pattern" width="3" height="3" patternUnits="userSpaceOnUse"><rect width="2" height="2" fill="url(#linear)"/></pattern>
    <clipPath id="clip2"><rect width="95" height="95"/></clipPath>
    <clipPath id="clip1" clip-path="url(#clip2)"><rect width="90" height="90"/></clipPath>
    <mask id="mask2"><rect width="95" height="95" fill="white"/></mask>
    <mask id="mask1" mask="url(#mask2)"><rect width="90" height="90" fill="white"/></mask>
    <filter id="all" x="0" y="0" width="100" height="100" filterUnits="userSpaceOnUse">
    <feBlend in="SourceGraphic" in2="SourceAlpha" result="blend"/>
    <feComposite in="blend" in2="SourceGraphic" operator="over" result="composite"/>
    <feDisplacementMap in="composite" in2="SourceAlpha" scale="1" result="displace"/>
    <feColorMatrix in="displace" type="matrix" values="1 0 0 0 0 0 1 0 0 0 0 0 1 0 0 0 0 0 1 0" result="matrix"/>
    <feComponentTransfer in="matrix" result="transfer"><feFuncR type="table" tableValues="0 .5 1"/><feFuncG type="discrete" tableValues="0 1"/><feFuncB type="linear" slope="1"/><feFuncA type="gamma" amplitude="1" exponent="1"/></feComponentTransfer>
    <feConvolveMatrix in="transfer" order="3" kernelMatrix="0 0 0 0 1 0 0 0 0" result="convolve"/>
    <feDiffuseLighting in="convolve" result="diffuse"><feDistantLight azimuth="45" elevation="45"/></feDiffuseLighting>
    <feSpecularLighting in="diffuse" result="specular"><feDistantLight azimuth="45" elevation="45"/></feSpecularLighting>
    <feDropShadow in="specular" stdDeviation="1" result="shadow"/>
    <feGaussianBlur in="shadow" stdDeviation="1" result="blur"/>
    <feImage href="#inner" result="image"/>
    <feMerge result="merge">"##.to_string();
    for _ in 0..300 {
        source.push_str("<feMergeNode in=\"blend\"/>");
    }
    source.push_str(r##"</feMerge><feMorphology in="merge" radius="1" result="morph"/><feOffset in="morph" dx="1" result="offset"/><feTile in="offset" result="tile"/><feFlood result="flood"/><feTurbulence baseFrequency=".1" result="turbulence"/></filter>
    </defs><g clip-path="url(#clip1)" mask="url(#mask1)" filter="url(#all)">
    <rect id="one" width="20" height="20" fill="url(#linear)" stroke="url(#radial)" stroke-dasharray="1 2"/>
    <rect id="two" x="25" width="20" height="20" fill="url(#pattern)"/>
    <rect id="three" x="50" width="20" height="20" fill="url(#linear)"/>
    </g></svg>"##);
    source
}

#[test]
fn all_scene_resources_and_merge_items_equal_original_recursive_estimate() {
    let tree = plain_tree(&scene_source());
    assert!(!tree.linear_gradients.is_empty());
    assert!(!tree.radial_gradients.is_empty());
    assert!(!tree.patterns.is_empty());
    assert!(!tree.clip_paths.is_empty());
    assert!(!tree.masks.is_empty());
    assert!(!tree.filters.is_empty());
    let filter = &tree.filters[0];
    assert!(filter.primitives.len() >= 15);
    use crate::filter::Kind;
    let kinds: Vec<_> = filter
        .primitives
        .iter()
        .map(|p| match &p.kind {
            Kind::Blend(_) => "blend",
            Kind::Composite(_) => "composite",
            Kind::DisplacementMap(_) => "displacement",
            Kind::ColorMatrix(_) => "matrix",
            Kind::ComponentTransfer(_) => "transfer",
            Kind::ConvolveMatrix(_) => "convolve",
            Kind::DiffuseLighting(_) => "diffuse",
            Kind::SpecularLighting(_) => "specular",
            Kind::DropShadow(_) => "shadow",
            Kind::GaussianBlur(_) => "blur",
            Kind::Image(_) => "image",
            Kind::Merge(_) => "merge",
            Kind::Morphology(_) => "morph",
            Kind::Offset(_) => "offset",
            Kind::Tile(_) => "tile",
            Kind::Flood(_) => "flood",
            Kind::Turbulence(_) => "turbulence",
        })
        .collect();
    for expected in [
        "blend",
        "composite",
        "displacement",
        "matrix",
        "transfer",
        "convolve",
        "diffuse",
        "specular",
        "shadow",
        "blur",
        "image",
        "merge",
        "morph",
        "offset",
        "tile",
        "flood",
        "turbulence",
    ] {
        assert!(
            kinds.contains(&expected),
            "missing real filter branch {expected}: {kinds:?}"
        );
    }
    assert!(filter
        .primitives
        .iter()
        .any(|p| matches!(&p.kind,crate::filter::Kind::Merge(v)if v.inputs.len()==300)));
    let plain = ProcessingContext::default();
    let expected = oracle::group_bytes(tree.root(), 0);
    assert!(expected.unwrap() > size_of::<Group>());
    assert_eq!(
        estimate(Frame::Group(tree.root(), 0), &plain).unwrap(),
        expected
    );
    let (r, state) = crate::geometry_test_support::request(Default::default());
    let paid = ProcessingContext::with_resources(r.clone());
    assert_eq!(
        estimate(Frame::Group(tree.root(), 0), &paid).unwrap(),
        expected
    );
    assert_eq!(state.live_len(), 0);
    drop(paid);
    r.validate_retired().unwrap();
    for depth in [255, 256, 257] {
        assert_eq!(
            estimate(Frame::Group(tree.root(), depth), &plain).unwrap(),
            oracle::group_bytes(tree.root(), depth)
        );
        for node in &tree.root.children {
            assert_eq!(
                estimate(Frame::Node(node, depth), &plain).unwrap(),
                oracle::node_bytes(node, depth)
            );
        }
    }
    let payload = LocalPayload::Colr(tree.into_root_in(&plain).unwrap());
    assert_eq!(
        payload_bytes(&payload, &plain).unwrap(),
        oracle::payload_bytes(&payload)
    );
}

fn image(kind: crate::ImageKind, p: &ProcessingContext) -> crate::Image {
    crate::Image {
        origin: crate::tree::owner::Origin::new(p),
        id: crate::tree::owner::TreeString::new(p),
        visible: true,
        size: crate::Size::from_wh(2., 2.).unwrap(),
        rendering_mode: crate::ImageRendering::OptimizeQuality,
        kind,
        abs_transform: crate::Transform::default(),
        abs_bounding_box: crate::NonZeroRect::from_xywh(0., 0., 2., 2.).unwrap(),
    }
}
#[test]
fn encoded_kinds_and_repeated_references_keep_original_coarse_values() {
    let p = ProcessingContext::default();
    for which in 0..4 {
        let bytes: crate::EncodedImageData = Arc::new(vec![1u8; 257]).into();
        let kind = match which {
            0 => crate::ImageKind::PNG(bytes),
            1 => crate::ImageKind::JPEG(bytes),
            2 => crate::ImageKind::GIF(bytes),
            _ => crate::ImageKind::WEBP(bytes),
        };
        let image = image(kind, &p);
        assert_eq!(
            estimate(Frame::Image(&image), &p).unwrap(),
            oracle::image_bytes(&image)
        );
    }
    let tree = plain_tree(&scene_source());
    for resource in &tree.root.filters {
        assert_eq!(
            estimate(Frame::Filter(resource, 0), &p).unwrap(),
            oracle::filter_bytes(resource, 0)
        );
    }
}

#[test]
fn nested_svg_resets_depth_without_new_global_cap_and_retires_real_frame_storage() {
    use crate::tree::owner::TreeVec;
    let p = ProcessingContext::default();
    let mut tree = plain_tree(r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"/>"#);
    for _ in 0..300 {
        let mut outer =
            plain_tree(r#"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"/>"#);
        let nested = image(crate::ImageKind::SVG(tree), &p);
        let boxed = crate::scene::boxed_owned(&p, crate::scene::SceneCost::NODE, nested).unwrap();
        // The test fixture is plain. Avoid an unrelated full-tree validation on every append.
        match &mut outer.root.children {
            TreeVec::Plain(v) => v.push(Node::Image(boxed)),
            _ => unreachable!(),
        }
        tree = outer;
    }
    let expected = oracle::group_bytes(tree.root(), 0);
    assert!(expected.is_some());
    let (r, state) = crate::geometry_test_support::request(Default::default());
    assert_eq!(
        estimate(
            Frame::Group(tree.root(), 0),
            &ProcessingContext::with_resources(r.clone())
        )
        .unwrap(),
        expected
    );
    assert!(state.counts().reserve > 1);
    assert_eq!(state.live_len(), 0);
    r.validate_retired().unwrap();
    // The production whole-Tree recursive Drop remains outside this task.
    while let Some(Node::Image(mut child)) = tree.root.children.pop() {
        let kind = std::mem::replace(
            &mut child.as_mut().kind,
            crate::ImageKind::PNG(Arc::new(Vec::new()).into()),
        );
        match kind {
            crate::ImageKind::SVG(next) => tree = next,
            _ => panic!("nested fixture"),
        }
    }
}

#[test]
fn cancellations_all_stack_allocations_and_last_refund_are_typed_before_return() {
    use crate::geometry_test_support::{cancelled, native, request, Call};
    let tree = plain_tree(&scene_source());
    let (r, state) = request(Default::default());
    estimate(
        Frame::Group(tree.root(), 0),
        &ProcessingContext::with_resources(r.clone()),
    )
    .unwrap()
    .unwrap();
    let counts = state.counts();
    assert!(
        counts.reserve > 0 && counts.commit > 0 && counts.release > 0 && counts.checkpoint > 256
    );
    for (call, count) in [
        (Call::Reserve, counts.reserve),
        (Call::Commit, counts.commit),
    ] {
        for ordinal in 1..=count {
            let (r, state) = request(Default::default());
            state.arm(call, ordinal, native(-991));
            assert_eq!(
                estimate(
                    Frame::Group(tree.root(), 0),
                    &ProcessingContext::with_resources(r.clone())
                ),
                Err(native(-991))
            );
            assert_eq!(r.failure(), Some(native(-991)));
            assert_eq!(state.live_len(), 0);
            r.validate_retired().unwrap();
        }
    }
    for ordinal in [1, 256, 257, counts.checkpoint - 1, counts.checkpoint] {
        let (r, state) = request(Default::default());
        state.arm(Call::Checkpoint, ordinal, cancelled());
        assert_eq!(
            estimate(
                Frame::Group(tree.root(), 0),
                &ProcessingContext::with_resources(r.clone())
            ),
            Err(cancelled())
        );
        assert_eq!(state.live_len(), 0);
        r.validate_retired().unwrap();
    }
    let (r, state) = request(Default::default());
    state.arm(Call::Release, counts.release, native(-992));
    assert_eq!(
        estimate(
            Frame::Group(tree.root(), 0),
            &ProcessingContext::with_resources(r.clone())
        ),
        Err(native(-992))
    );
    assert_eq!(r.failure(), Some(native(-992)));
    assert_eq!(r.statistics().release_failures, 1);
    assert!(r.validate_retired().is_err());
}

#[test]
fn real_work_quota_precedes_cleanup_error_and_frees_physical_stack() {
    use crate::geometry_test_support::{native, request, AllocationObservation, Call};
    let tree = plain_tree(&scene_source());
    let (r, state) = request(Default::default());
    let observed = AllocationObservation::begin();
    estimate(
        Frame::Group(tree.root(), 0),
        &ProcessingContext::with_resources(r.clone()),
    )
    .unwrap()
    .unwrap();
    let allocations = observed.finish();
    assert!(allocations.alloc + allocations.zeroed > 0);
    assert_eq!(allocations.alloc + allocations.zeroed, allocations.dealloc);
    let mut paid_work = 0;
    let mut quota = None;
    for event in &state.lock().events {
        if event.call == Call::Checkpoint {
            if event.live_bytes > 0 && event.work > 0 {
                quota = Some(paid_work);
                break;
            }
            paid_work += event.work;
        }
    }
    let mut limits = markdown_svg_budget::Limits::default();
    limits.work = quota.expect("actual paid-stack action");
    let (r, state) = request(limits);
    state.arm(Call::Release, 1, native(-993));
    let observed = AllocationObservation::begin();
    let result = estimate(
        Frame::Group(tree.root(), 0),
        &ProcessingContext::with_resources(r.clone()),
    );
    let allocations = observed.finish();
    let error = result.unwrap_err();
    assert_eq!(error.kind, markdown_svg_budget::FailureKind::Quota);
    assert_eq!(error.limit, markdown_svg_budget::LimitKind::Work);
    assert_eq!(r.failure(), Some(error));
    assert_eq!(r.statistics().release_failures, 1);
    assert!(allocations.alloc + allocations.zeroed > 0);
    assert_eq!(allocations.alloc + allocations.zeroed, allocations.dealloc);
    assert!(r.validate_retired().is_err());
}

#[test]
fn unsupported_text_and_zero_payload_keep_the_original_missing_result() {
    let mut db = fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    let opt = crate::Options {
        fontdb: Arc::new(db),
        font_family: "Tuffy".to_owned(),
        ..crate::Options::default()
    };
    let tree=crate::Tree::from_str(r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text x="10" y="30" font-family="Tuffy">A</text></svg>"#,&opt).unwrap();
    assert!(tree
        .root
        .children
        .iter()
        .any(|n| matches!(n, Node::Text(_))));
    let p = ProcessingContext::default();
    assert_eq!(oracle::group_bytes(tree.root(), 0), None);
    assert_eq!(estimate(Frame::Group(tree.root(), 0), &p).unwrap(), None);
    let (r, state) = crate::geometry_test_support::request(Default::default());
    assert_eq!(
        estimate(
            Frame::Group(tree.root(), 0),
            &ProcessingContext::with_resources(r.clone())
        )
        .unwrap(),
        None
    );
    assert_eq!(state.live_len(), 0);
    r.validate_retired().unwrap();
    let empty = LocalPayload::Ordinary(None);
    assert_eq!(
        payload_bytes(&empty, &p).unwrap(),
        oracle::payload_bytes(&empty)
    );
}
