// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual nested entry, before any configuration copy or forwarding closure.
use super::*;
use crate::geometry_test_support::{AllocationObservation, Call, native, request};
use markdown_svg_budget::{FailureKind, LimitKind};
use std::sync::Arc;

#[test]
fn nested_identity_and_depth_rejection_precede_any_configuration_system_allocation() {
    for foreign in [false, true] {
        let (source, source_state) = request(Default::default());
        let (target, target_state) = request(Default::default());
        let source_r = if foreign { &source } else { &target };
        let data = crate::EncodedImageData::try_copy_from_slice(
            source_r,
            b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>",
        )
        .unwrap();
        let mut options = Options::default();
        options.processing = Arc::new(crate::ProcessingContext::with_resources(target.clone()));
        options.languages = vec!["en-US".repeat(300), "zh-CN".repeat(200)];
        let scopes = if foreign {
            Vec::new()
        } else {
            (0..8)
                .map(|_| target.enter_svg_depth().unwrap())
                .collect::<Vec<_>>()
        };
        let before = target_state.counts();
        let observation = AllocationObservation::begin();
        let result = crate::Tree::from_data_nested(&data, &options);
        let allocations = observation.finish();
        assert!(result.is_err());
        let error = target.failure().unwrap();
        assert_eq!(
            (error.kind, error.limit),
            if foreign {
                (FailureKind::Invariant, LimitKind::None)
            } else {
                (FailureKind::Quota, LimitKind::SvgDepth)
            }
        );
        assert_eq!(
            allocations.alloc + allocations.zeroed + allocations.realloc,
            0,
            "foreign={foreign}"
        );
        assert_eq!(target_state.counts().reserve, before.reserve);
        drop((result, scopes, options, data));
        assert_eq!(source_state.live_len(), 0);
        assert_eq!(target_state.live_len(), 0);
        source.validate_retired().unwrap();
        target.validate_retired().unwrap();
    }
}

fn dynamic_options(resources: &markdown_svg_budget::ResourcesRef) -> Options<'static> {
    let mut options = Options::default();
    options.processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
    options.dpi = 144.0;
    options.font_size = 27.0;
    options.font_family = "parent-family".to_owned();
    options.languages = vec!["x".repeat(17011), "中".repeat(4099)];
    options
}

fn configuration_bytes(options: &Options) -> usize {
    "Times New Roman".len()
        + options.languages.capacity() * std::mem::size_of::<String>()
        + options.languages.iter().map(|s| s.len()).sum::<usize>()
}

#[test]
fn nested_dynamic_configuration_is_funded_and_callbacks_are_borrowed() {
    let (resources, state) = request(Default::default());
    let options = dynamic_options(&resources);
    let expected = configuration_bytes(&options);
    let observation = AllocationObservation::begin();
    let child = options.nested_image_options().unwrap();
    let allocations = observation.finish();
    assert_eq!(child.dpi, 144.0);
    assert_eq!(child.font_size, 27.0);
    assert_eq!(child.font_family, "Times New Roman");
    assert_eq!(child.languages, options.languages);
    assert!(matches!(
        &child.image_href_resolver.resolve_data,
        crate::ImageHrefDataResolverFn::Borrowed(_)
    ));
    assert!(matches!(
        &child.image_href_resolver.resolve_string,
        crate::ImageHrefStringResolverFn::Borrowed(_)
    ));
    assert!(matches!(
        &child.font_resolver.select_font,
        crate::FontSelectionFn::Borrowed(_)
    ));
    assert!(matches!(
        &child.font_resolver.select_fallback,
        crate::FallbackSelectionFn::Default
    ));
    assert_eq!(
        allocations.alloc + allocations.zeroed + allocations.realloc,
        2 + options.languages.len()
    );
    assert_eq!(resources.statistics().scratch_bytes, expected as u64);
    assert_eq!(state.counts().reserve, 1);
    assert_eq!(state.counts().commit, 1);
    drop(child);
    assert_eq!(state.live_len(), 0);
    drop(options);
    resources.validate_retired().unwrap();
}

#[test]
fn actual_nested_configuration_refund_failure_prevents_next_tree_publication() {
    let mut point = None;
    for fault in [false, true] {
        let (resources, state) = request(Default::default());
        let options = dynamic_options(&resources);
        let data = crate::EncodedImageData::try_copy_from_slice(
            &resources,
            b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>",
        )
        .unwrap();
        let bytes = configuration_bytes(&options);
        let before = state.counts();
        if fault {
            state.arm(Call::Release, point.unwrap(), native(-22202));
        }
        let result = crate::Tree::from_data_nested(&data, &options);
        let mut consumers = 0;
        if result.is_ok() {
            consumers += 1;
        }
        if fault {
            assert_eq!(consumers, 0);
            assert_eq!(resources.failure(), Some(native(-22202)));
            assert!(
                matches!(&result,Err(Error::ProcessingFailure(e)) if e.typed_failure==native(-22202))
            );
            assert_eq!(resources.statistics().release_failures, 1);
        } else {
            assert_eq!(consumers, 1);
            let events = state.lock();
            assert_eq!(
                events
                    .events
                    .iter()
                    .filter(|e| e.call == Call::Reserve && e.bytes == bytes)
                    .count(),
                1,
                "unique real configuration ticket"
            );
            let releases = events
                .events
                .iter()
                .filter(|e| e.call == Call::Release && e.bytes == bytes)
                .collect::<Vec<_>>();
            assert_eq!(releases.len(), 1);
            point = Some(releases[0].ordinal - before.release);
        }
        drop((result, data, options));
        if fault {
            assert_eq!(state.live_len(), 1);
            assert_eq!(resources.statistics().scratch_bytes, bytes as u64);
            assert!(resources.validate_retired().is_err());
        } else {
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn nested_configuration_reserve_and_commit_faults_stop_before_xml_producer() {
    for call in [Call::Reserve, Call::Commit] {
        let (resources, state) = request(Default::default());
        let options = dynamic_options(&resources);
        let data = crate::EncodedImageData::try_copy_from_slice(
            &resources,
            b"<svg xmlns='http://www.w3.org/2000/svg' width='4' height='4'/>",
        )
        .unwrap();
        state.arm(call, 1, native(-22203));
        let observation = AllocationObservation::begin();
        let result = crate::Tree::from_data_nested(&data, &options);
        let allocations = observation.finish();
        assert!(matches!(result,Err(Error::Resource(e)) if e==native(-22203)));
        assert_eq!(
            options.processing.counts().0,
            0,
            "no XML/scene producer after configuration failure"
        );
        if call == Call::Reserve {
            assert_eq!(
                allocations.alloc + allocations.zeroed + allocations.realloc,
                0
            );
        } else {
            assert_eq!(
                allocations.alloc + allocations.zeroed + allocations.realloc,
                allocations.dealloc
            );
        }
        assert_eq!(resources.failure(), Some(native(-22203)));
        drop((data, options));
        assert_eq!(state.live_len(), 0);
        resources.validate_retired().unwrap();
    }
}
