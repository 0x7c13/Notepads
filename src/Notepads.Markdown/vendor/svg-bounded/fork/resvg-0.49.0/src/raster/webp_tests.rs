// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real renderer integration with fixed upstream pixels and animation input.
use super::*;
use base64::Engine;

const RGB_2: &[u8] = include_bytes!("fixtures/upstream-red-2x2.webp");
const RGB_3: &[u8] = include_bytes!("fixtures/upstream-red-3x3.webp");
const ALPHA: &[u8] = include_bytes!("fixtures/upstream-color-index-30x30.webp");
const ALPHA_EXPECTED: &[u8] =
    include_bytes!("fixtures/upstream-color-index-30x30-premultiplied.rgba");
const ANIMATED: &[u8] = include_bytes!("fixtures/upstream-random-lossy.webp");

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
fn body(encoded: &[u8]) -> String {
    format!(
        "<image href='{}' width='64' height='64' image-rendering='pixelated'/>",
        data_url("image/webp", encoded)
    )
}
fn parsed(encoded: &[u8]) -> (usvg::Tree, ResourcesRef, Arc<State>) {
    let (resources, state) = request(8 << 20);
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
        body(encoded)
    );
    let tree = usvg::Tree::from_data_controlled(source.as_bytes(), &options).unwrap();
    (tree, resources, state)
}

#[test]
fn actual_webp_rgb_alpha_images_nested_svg_and_feimage_match_compatibility_pixels() {
    for encoded in [RGB_2, RGB_3, ALPHA] {
        let href = data_url("image/webp", encoded);
        let nested = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'>{}</svg>",
            body(encoded)
        );
        for source in [
            body(encoded),
            format!("<image href='{}' width='64' height='64'/>",data_url("image/svg+xml",nested.as_bytes())),
            format!("<defs><filter id='f' x='0' y='0' width='64' height='64' filterUnits='userSpaceOnUse'><feImage href='{href}' width='64' height='64'/></filter></defs><rect width='64' height='64' filter='url(#f)'/>"),
        ] {
            let (expected,compatibility_ok,_)=render_document(&source,false,None);
            let (actual,controlled_ok,_)=render_document(&source,true,None);
            assert!(compatibility_ok && controlled_ok);
            assert!(actual.chunks_exact(4).any(|pixel|pixel[3]!=0));
            assert_eq!(actual,expected);
        }
    }
}

#[test]
fn webp_bridge_preserves_frozen_partial_alpha_raw_and_original_rgb_expansion() {
    assert_eq!(ALPHA_EXPECTED.len(), 30 * 30 * 4);
    assert!(ALPHA_EXPECTED
        .chunks_exact(4)
        .any(|p| p[3] > 0 && p[3] < 255));
    let (resources, state) = request(8 << 20);
    let image = crate::raster_decode::decode_webp_controlled(ALPHA, &resources).unwrap();
    assert_eq!((image.width(), image.height()), (30, 30));
    // This is the original pre-implementation full raw baseline, not a new
    // decoder-generated expectation or integer-premultiply approximation.
    assert_eq!(image.data(), ALPHA_EXPECTED);
    assert_eq!(state.used.load(Ordering::SeqCst), 30 * 30 * 4);
    drop(image);
    resources.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    for (encoded, width) in [(RGB_2, 2u32), (RGB_3, 3u32)] {
        let mut original = image_webp::WebPDecoder::new(std::io::Cursor::new(encoded)).unwrap();
        assert!(!original.has_alpha());
        let mut rgb = vec![0u8; original.output_buffer_size().unwrap()];
        original.read_image(&mut rgb).unwrap();
        let (resources, state) = request(8 << 20);
        let image = crate::raster_decode::decode_webp_controlled(encoded, &resources).unwrap();
        assert_eq!((image.width(), image.height()), (width, width));
        for (rgba, rgb) in image.data().chunks_exact(4).zip(rgb.chunks_exact(3)) {
            assert_eq!(&rgba[..3], rgb);
            assert_eq!(rgba[3], 255);
        }
        drop(image);
        resources.validate_retired().unwrap();
        assert_eq!(state.used.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn webp_target_refusal_keeps_native_error_and_fresh_request_recovers() {
    let (resources, state) = request(8 << 20);
    state
        .reject_reserve_bytes
        .store(3 * 3 * 4, Ordering::SeqCst);
    let error = crate::raster_decode::decode_webp_controlled(RGB_3, &resources).unwrap_err();
    assert_eq!(error, Error::native(-778));
    assert_eq!(resources.failure(), Some(error));
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
    let (fresh, state) = request(8 << 20);
    let image = crate::raster_decode::decode_webp_controlled(RGB_3, &fresh).unwrap();
    assert!(image.data().chunks_exact(4).all(|pixel| pixel[3] == 255));
    drop(image);
    fresh.validate_retired().unwrap();
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn renderer_keeps_webp_target_late_release_failure_after_real_drawing() {
    let (tree, resources, state) = parsed(RGB_3);
    state
        .reject_release_bytes
        .store(3 * 3 * 4, Ordering::SeqCst);
    let mut target = tiny_skia::Pixmap::new(64, 64).unwrap();
    let error = crate::render_in_request(
        &tree,
        tiny_skia::Transform::identity(),
        &mut target.as_mut(),
    )
    .unwrap_err();
    assert_eq!(error.typed_failure, Error::native(-777));
    assert_eq!(state.rejected_releases.load(Ordering::SeqCst), 1);
    assert!(target.data().chunks_exact(4).any(|pixel| pixel[3] != 0));
    drop(tree);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    assert_eq!(resources.statistics().release_failures, 1);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn webp_bridge_cancellation_sweep_never_publishes_and_retires_paid_owners() {
    for encoded in [RGB_3, ALPHA] {
        let (resources, state) = request(8 << 20);
        let image = crate::raster_decode::decode_webp_controlled(encoded, &resources).unwrap();
        let checkpoints = state.calls.load(Ordering::SeqCst);
        drop(image);
        resources.validate_retired().unwrap();
        assert!(checkpoints > 5);
        let mut after_actual_reserve = 0;
        for call in 0..checkpoints {
            let (resources, state) = request(8 << 20);
            state.cancel_at.store(call, Ordering::SeqCst);
            let error =
                crate::raster_decode::decode_webp_controlled(encoded, &resources).unwrap_err();
            assert_eq!(error.kind, FailureKind::Cancelled);
            assert_eq!(resources.failure(), Some(error));
            after_actual_reserve += usize::from(state.next.load(Ordering::SeqCst) > 0);
            assert_eq!(state.used.load(Ordering::SeqCst), 0);
            resources.validate_retired().unwrap();
        }
        assert!(after_actual_reserve > 0);
    }
}

#[test]
fn webp_header_dimension_gate_rejects_before_target_or_pixel_decode_buffers() {
    let mut encoded = RGB_2.to_vec();
    // Metadata-only derived boundary from the fixed original VP8 keyframe:
    // bytes26..30 are 14-bit little-endian dimensions. Entropy is unchanged;
    // no decoded pixels or valid full-image claim is attached to this input.
    assert_eq!(&encoded[23..26], &[0x9d, 0x01, 0x2a]);
    encoded[26..28].copy_from_slice(&4097u16.to_le_bytes());
    encoded[28..30].copy_from_slice(&4096u16.to_le_bytes());
    let (resources, state) = request(8 << 20);
    let error = crate::raster_decode::decode_webp_controlled(&encoded, &resources).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Capacity));
    assert_eq!(resources.failure(), Some(error));
    assert!(state.peak.load(Ordering::SeqCst) < 1 << 20);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn original_webp_animation_is_unsupported_before_pixel_buffers_and_retires() {
    let (resources, state) = request(8 << 20);
    let error = crate::raster_decode::decode_webp_controlled(ANIMATED, &resources).unwrap_err();
    assert_eq!(error.kind, FailureKind::Unsupported);
    assert_eq!(resources.failure(), Some(error));
    assert!(state.peak.load(Ordering::SeqCst) < 1 << 20);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
    resources.validate_retired().unwrap();
}
